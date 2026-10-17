//! The libcosmic app: a thin layer that shows `State`, feeds `Msg`s to
//! `state::update`, and runs the effects it returns.

mod apply_sheet;
mod caption;
mod compare;
mod effects;
mod header;
mod io_queue;
mod preview;
mod roles;
mod scene;
#[cfg(test)]
mod shots;
mod state;
mod wallpaper;

use std::path::PathBuf;

use cosmic::app::{Core, Settings, Task};
use cosmic::dialog::file_chooser;
use cosmic::iced::event::{self, Event};
use cosmic::iced::keyboard;
use cosmic::iced::{Length, Limits, Size, Subscription, window};
use cosmic::widget::menu::KeyBind;
use cosmic::widget::menu::key_bind::Modifier;
use cosmic::widget::{self, ToastId, Toasts, text};
use cosmic::{Application, ApplicationExt, Element};
use irida_core::apply::Ctx;

use effects::perform_caught;
use io_queue::IoQueue;
use state::{Current, Effect, FolderFor, Msg, State, ToastItem, update};

pub const APP_ID: &str = "io.github.atraxsrc.Irida";

/// Where COSMIC keeps its system defaults, for the wallpaper reader.
const COSMIC_SYSTEM: &str = "/usr/share/cosmic";

#[derive(Debug, Clone)]
pub enum Message {
    Ui(Box<Msg>),
    /// The Msgs a batch of IO effects gave, in order. Sent only when a
    /// batch finishes, since it starts the next one.
    Results(Vec<Msg>),
    /// A file chooser was cancelled.
    Nothing,
    CloseToast(ToastId),
}

pub struct App {
    core: Core,
    state: State,
    toasts: Toasts<Message>,
    /// IO effects run one batch at a time, in the order they were asked for.
    io: IoQueue<Effect>,
}

/// Open the window. The only place the app reads its folders and `$PATH`
/// from the environment.
pub fn run() -> Result<(), String> {
    let ctx = Ctx::for_app().map_err(|e| e.to_string())?;
    let settings = Settings::default()
        .size(Size::new(1200., 780.))
        .size_limits(Limits::NONE.min_width(720.).min_height(480.))
        .exit_on_close(false);
    cosmic::app::run::<App>(settings, ctx).map_err(|e| e.to_string())
}

impl App {
    fn handle(&mut self, msg: Msg) -> Task<Message> {
        let effects = update(&mut self.state, msg);
        self.run_effects(effects)
    }

    /// Toasts and timers happen here; the IO effects queue behind any still
    /// running and run in order on a blocking thread, so a later effect sees
    /// an earlier one's writes, even from an earlier `update`. Exit waits
    /// until the queue is empty.
    fn run_effects(&mut self, effects: Vec<Effect>) -> Task<Message> {
        let mut tasks = Vec::new();
        let mut io = Vec::new();
        let mut exit = false;
        for effect in effects {
            match effect {
                Effect::Toast(item) => tasks.push(self.toast(item)),
                Effect::Copy(text) => tasks.push(cosmic::iced::clipboard::write(text)),
                Effect::ChooseFolder(purpose) => tasks.push(choose_folder(purpose)),
                Effect::ChooseImage => tasks.push(choose_image()),
                Effect::StartTimer(delay, generation) => {
                    tasks.push(cosmic::task::future(async move {
                        tokio::time::sleep(delay).await;
                        Message::Ui(Box::new(Msg::LiveTick(generation)))
                    }));
                }
                Effect::Exit => exit = true,
                effect => {
                    debug_assert!(effect.is_io());
                    io.push(effect);
                }
            }
        }
        if let Some(batch) = self.io.enqueue(io) {
            tasks.push(self.start_io(batch));
        }
        // Exit waits for queued IO, so a save asked for first still runs.
        if exit && self.io.exit() {
            tasks.push(cosmic::iced::exit());
        }
        Task::batch(tasks)
    }

    /// Run one batch of IO effects; `Message::Results` reports it finished.
    /// An effect that panics gives its own failure Msg.
    fn start_io(&self, batch: Vec<Effect>) -> Task<Message> {
        let ctx = self.state.ctx.clone();
        cosmic::task::future(async move {
            let msgs = tokio::task::spawn_blocking(move || {
                batch
                    .into_iter()
                    .flat_map(|e| perform_caught(e, &ctx))
                    .collect::<Vec<_>>()
            })
            .await
            .unwrap_or_default();
            Message::Results(msgs)
        })
    }

    fn toast(&mut self, item: ToastItem) -> Task<Message> {
        let mut toast = widget::Toast::new(item.text);
        if let Some((label, msg)) = item.action {
            toast = toast.action(label, move |_| Message::Ui(Box::new(msg.clone())));
        }
        self.toasts.push(toast).map(cosmic::Action::App)
    }
}

/// Ask for a folder with the portal's file chooser; cancelling sends
/// nothing.
fn choose_folder(purpose: FolderFor) -> Task<Message> {
    cosmic::task::future(async move {
        let dialog = file_chooser::open::Dialog::new().title(purpose.title());
        let result = match dialog.open_folder().await {
            Ok(response) => response
                .0
                .uris()
                .first()
                .and_then(|url| url.to_file_path().ok())
                .ok_or_else(|| "the chosen folder is not a local folder".to_string()),
            Err(file_chooser::Error::Cancelled) => return Message::Nothing,
            Err(e) => Err(e.to_string()),
        };
        Message::Ui(Box::new(Msg::Folder(purpose, result)))
    })
}

/// The images a wallpaper draft reads, as file chooser patterns (both
/// cases, since the portal's patterns are case sensitive).
const IMAGE_PATTERNS: [&str; 8] = [
    "*.jpg", "*.jpeg", "*.png", "*.webp", "*.JPG", "*.JPEG", "*.PNG", "*.WEBP",
];

/// Ask for an image with the portal's file chooser; cancelling sends
/// nothing.
fn choose_image() -> Task<Message> {
    cosmic::task::future(async move {
        let filter = IMAGE_PATTERNS
            .iter()
            .fold(file_chooser::FileFilter::new("Images"), |f, p| f.glob(p));
        let dialog = file_chooser::open::Dialog::new()
            .title("Choose an image")
            .filter(filter);
        let result = match dialog.open_file().await {
            Ok(response) => response
                .0
                .uris()
                .first()
                .and_then(|url| url.to_file_path().ok())
                .ok_or_else(|| "the chosen file is not a local file".to_string()),
            Err(file_chooser::Error::Cancelled) => return Message::Nothing,
            Err(e) => Err(e.to_string()),
        };
        Message::Ui(Box::new(Msg::Image(result)))
    })
}

/// Ctrl+S saves; a compositor close asks first, as the header button does.
/// Focus is reported, so a second window can try the app's lock again.
fn events(event: Event, _status: event::Status, _window: window::Id) -> Option<Message> {
    match event {
        Event::Keyboard(keyboard::Event::KeyPressed {
            key,
            physical_key,
            modifiers,
            ..
        }) => {
            let save = KeyBind {
                modifiers: vec![Modifier::Ctrl],
                key: keyboard::Key::Character("s".into()),
            };
            save.matches(modifiers, &key, Some(&physical_key))
                .then_some(Message::Ui(Box::new(Msg::Save)))
        }
        Event::Window(window::Event::CloseRequested) => {
            Some(Message::Ui(Box::new(Msg::CloseRequested)))
        }
        Event::Window(window::Event::Focused) => Some(Message::Ui(Box::new(Msg::Focused))),
        _ => None,
    }
}

impl Application for App {
    type Executor = cosmic::executor::Default;
    type Flags = Ctx;
    type Message = Message;
    const APP_ID: &'static str = APP_ID;

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, ctx: Ctx) -> (Self, Task<Message>) {
        let lock = irida_core::live::lock_app(&ctx.dirs);
        let (state, effects) = State::new(ctx, PathBuf::from(COSMIC_SYSTEM), lock);
        let mut app = App {
            core,
            state,
            toasts: Toasts::new(Message::CloseToast),
            io: IoQueue::default(),
        };
        let mut tasks = vec![app.run_effects(effects)];
        if let Some(id) = app.core.main_window_id() {
            tasks.push(app.set_window_title("Irida".to_string(), id));
        }
        (app, Task::batch(tasks))
    }

    fn header_start(&self) -> Vec<Element<'_, Message>> {
        header::start(&self.state)
            .into_iter()
            .map(|e| e.map(|m| Message::Ui(Box::new(m))))
            .collect()
    }

    fn header_end(&self) -> Vec<Element<'_, Message>> {
        header::end(&self.state)
            .into_iter()
            .map(|e| e.map(|m| Message::Ui(Box::new(m))))
            .collect()
    }

    fn dialog(&self) -> Option<Element<'_, Message>> {
        header::dialog(&self.state).map(|e| e.map(|m| Message::Ui(Box::new(m))))
    }

    /// The header's close button asks first when there are unsaved changes.
    fn on_app_exit(&mut self) -> Option<Message> {
        Some(Message::Ui(Box::new(Msg::CloseRequested)))
    }

    fn on_escape(&mut self) -> Task<Message> {
        self.handle(Msg::CloseDialog)
    }

    fn subscription(&self) -> Subscription<Message> {
        event::listen_with(events)
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Ui(msg) => self.handle(*msg),
            Message::Results(msgs) => {
                // What was queued while this batch ran starts first; what
                // these Msgs ask for queues behind it.
                let mut tasks = Vec::new();
                if let Some(batch) = self.io.finished() {
                    tasks.push(self.start_io(batch));
                }
                for msg in msgs {
                    tasks.push(self.handle(msg));
                }
                if self.io.exit_due() {
                    tasks.push(cosmic::iced::exit());
                }
                Task::batch(tasks)
            }
            Message::Nothing => Task::none(),
            Message::CloseToast(id) => {
                self.toasts.remove(id);
                Task::none()
            }
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let content = widget::container(content(&self.state).map(|m| Message::Ui(Box::new(m))))
            .padding(16)
            .width(Length::Fill)
            .height(Length::Fill);
        widget::toaster(&self.toasts, content)
    }
}

/// Everything below the header: the role list and the preview, or why
/// there is nothing to edit.
fn content(state: &State) -> Element<'_, Msg> {
    match &state.current {
        Some(Current::Editable { working, .. }) => widget::Row::new()
            .spacing(24)
            .push(
                widget::container(roles::view(state, working))
                    .width(Length::FillPortion(2))
                    .max_width(480.),
            )
            .push(
                widget::Column::new()
                    .spacing(12)
                    .width(Length::FillPortion(3))
                    .push(
                        widget::scrollable(match state.split_view() {
                            Some(split) => compare::view(split),
                            None => preview::view(scene::scene(working)),
                        })
                        .height(Length::Fill),
                    )
                    .push(roles::notes(working)),
            )
            .into(),
        Some(Current::Unreadable { slug, error, text }) => header::unreadable(slug, error, text),
        None => text::body(state.empty_text()).into(),
    }
}
