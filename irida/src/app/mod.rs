//! The libcosmic app: a thin layer that shows `State`, feeds `Msg`s to
//! `state::update`, and runs the effects it returns.

mod effects;
mod header;
mod state;

use std::path::PathBuf;

use cosmic::app::{Core, Settings, Task};
use cosmic::iced::event::{self, Event};
use cosmic::iced::keyboard;
use cosmic::iced::{Background, Border, Color, Length, Limits, Size, Subscription, window};
use cosmic::widget::menu::KeyBind;
use cosmic::widget::menu::key_bind::Modifier;
use cosmic::widget::{self, ToastId, Toasts, text};
use cosmic::{Application, ApplicationExt, Element};
use irida_core::apply::Ctx;
use irida_core::palette::Palette;
use irida_core::role::Role;

use effects::perform;
use state::{Current, Effect, Msg, State, ToastItem, update};

pub const APP_ID: &str = "io.github.atraxsrc.Irida";

/// Where COSMIC keeps its system defaults, for the wallpaper reader.
const COSMIC_SYSTEM: &str = "/usr/share/cosmic";

#[derive(Debug, Clone)]
pub enum Message {
    Ui(Msg),
    /// The Msgs a batch of IO effects gave, in order.
    Results(Vec<Msg>),
    CloseToast(ToastId),
}

pub struct App {
    core: Core,
    state: State,
    toasts: Toasts<Message>,
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

    /// Toasts and exit happen here; the IO effects run in order on one
    /// blocking thread, so a later effect sees an earlier one's writes.
    fn run_effects(&mut self, effects: Vec<Effect>) -> Task<Message> {
        let mut tasks = Vec::new();
        let mut io = Vec::new();
        for effect in effects {
            match effect {
                Effect::Toast(item) => tasks.push(self.toast(item)),
                Effect::Exit => tasks.push(cosmic::iced::exit()),
                effect => {
                    debug_assert!(effect.is_io());
                    io.push(effect);
                }
            }
        }
        if !io.is_empty() {
            let ctx = self.state.ctx.clone();
            tasks.push(cosmic::task::future(async move {
                let msgs = tokio::task::spawn_blocking(move || {
                    io.into_iter()
                        .flat_map(|e| perform(e, &ctx))
                        .collect::<Vec<_>>()
                })
                .await
                .unwrap_or_default();
                Message::Results(msgs)
            }));
        }
        Task::batch(tasks)
    }

    fn toast(&mut self, item: ToastItem) -> Task<Message> {
        let mut toast = widget::Toast::new(item.text);
        if let Some((label, msg)) = item.action {
            toast = toast.action(label, move |_| Message::Ui(msg.clone()));
        }
        self.toasts.push(toast).map(cosmic::Action::App)
    }
}

/// Ctrl+S saves; a compositor close asks first, as the header button does.
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
                .then_some(Message::Ui(Msg::Save))
        }
        Event::Window(window::Event::CloseRequested) => Some(Message::Ui(Msg::CloseRequested)),
        _ => None,
    }
}

fn swatch(c: irida_core::color::Rgb) -> Element<'static, Msg> {
    widget::container(text::body(""))
        .width(Length::Fixed(28.))
        .height(Length::Fixed(28.))
        .style(move |_| widget::container::Style {
            background: Some(Background::Color(Color::from_rgb8(c.r, c.g, c.b))),
            border: Border {
                radius: 6.0.into(),
                width: 1.0,
                color: Color::from_rgba8(128, 128, 128, 0.5),
            },
            ..Default::default()
        })
        .into()
}

/// The roles at a glance; the role editor replaces this.
fn roles(p: &Palette) -> Element<'_, Msg> {
    let list = Role::ALL
        .iter()
        .fold(widget::Column::new().spacing(8), |col, role| {
            let c = p.roles.get(*role);
            let name = p
                .names
                .get(role)
                .map_or_else(|| role.as_str().to_string(), Clone::clone);
            col.push(
                widget::Row::new()
                    .spacing(12)
                    .align_y(cosmic::iced::Alignment::Center)
                    .push(swatch(c))
                    .push(text::body(name).width(Length::Fixed(160.)))
                    .push(text::monotext(c.to_hex())),
            )
        });
    widget::scrollable(list).into()
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
        let (state, effects) = State::new(ctx, PathBuf::from(COSMIC_SYSTEM));
        let mut app = App {
            core,
            state,
            toasts: Toasts::new(Message::CloseToast),
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
            .map(|e| e.map(Message::Ui))
            .collect()
    }

    fn header_end(&self) -> Vec<Element<'_, Message>> {
        header::end(&self.state)
            .into_iter()
            .map(|e| e.map(Message::Ui))
            .collect()
    }

    fn dialog(&self) -> Option<Element<'_, Message>> {
        header::dialog(&self.state).map(|e| e.map(Message::Ui))
    }

    /// The header's close button asks first when there are unsaved changes.
    fn on_app_exit(&mut self) -> Option<Message> {
        Some(Message::Ui(Msg::CloseRequested))
    }

    fn on_escape(&mut self) -> Task<Message> {
        self.handle(Msg::CloseDialog)
    }

    fn subscription(&self) -> Subscription<Message> {
        event::listen_with(events)
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Ui(msg) => self.handle(msg),
            Message::Results(msgs) => {
                let mut tasks = Vec::new();
                for msg in msgs {
                    tasks.push(self.handle(msg));
                }
                Task::batch(tasks)
            }
            Message::CloseToast(id) => {
                self.toasts.remove(id);
                Task::none()
            }
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let content: Element<'_, Msg> = match &self.state.current {
            Some(Current::Editable { working, .. }) => roles(working),
            Some(Current::Unreadable { slug, error, text }) => {
                header::unreadable(slug, error, text)
            }
            None => text::body("No palettes yet. Use New in the palette menu to make one.").into(),
        };
        let content = widget::container(content.map(Message::Ui))
            .padding(16)
            .width(Length::Fill)
            .height(Length::Fill);
        widget::toaster(&self.toasts, content)
    }
}
