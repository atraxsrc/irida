//! The applet itself: the panel icon, the popup, and the messages between
//! them and `actions`. What the popup shows is decided in `menu` and `copy`.

use std::fmt;
use std::time::Duration;

use cosmic::app::{Core, Task};
use cosmic::applet::token::subscription::{
    TokenRequest, TokenUpdate, activation_token_subscription,
};
use cosmic::cctk::sctk::reexports::calloop::channel::Sender;
use cosmic::iced::window::Id;
use cosmic::iced::{Alignment, Length, Rectangle, Subscription};
use cosmic::surface::action::{app_popup, destroy_popup};
use cosmic::widget::{self, text};
use cosmic::{Application, Element};
use irida_core::apply::Ctx;

use crate::actions::{self, Loaded};
use crate::copy::{self, Blocked};
use crate::menu::Row;
use crate::panel;

pub const APP_ID: &str = "io.github.atraxsrc.IridaApplet";
const ICON: &str = "io.github.atraxsrc.IridaApplet-symbolic";
const TICK: &str = "object-select-symbolic";
/// How long Open Irida waits for an activation token before starting the
/// window without one.
const TOKEN_WAIT: Duration = Duration::from_millis(800);

/// The channel to the compositor for activation tokens, once the
/// subscription has it.
#[derive(Clone)]
pub struct TokenTx(Sender<TokenRequest>);

impl fmt::Debug for TokenTx {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("TokenTx")
    }
}

pub struct Window {
    core: Core,
    ctx: Ctx,
    popup: Option<Id>,
    loaded: Option<Loaded>,
    /// An apply or undo in flight.
    busy: Option<Blocked>,
    status: Option<String>,
    token_tx: Option<TokenTx>,
    /// Open Irida was clicked and waits for a token or the timeout.
    opening: bool,
}

#[derive(Clone, Debug)]
pub enum Message {
    PopupClosed(Id),
    /// A row, by its index across pairs, dark, then light.
    Apply(usize),
    Applied(Result<String, String>),
    Undo,
    Undone(String),
    Open,
    TokenInit(TokenTx),
    TokenReady(Option<String>),
    TokenGone,
    OpenTimeout,
    Close,
    Remove,
    Surface(cosmic::surface::Action<Message>),
}

impl Window {
    fn rows(&self) -> Vec<&Row> {
        match &self.loaded {
            Some(l) => l
                .menu
                .pairs
                .iter()
                .chain(&l.menu.dark)
                .chain(&l.menu.light)
                .collect(),
            None => Vec::new(),
        }
    }

    /// Why the rows and Undo are disabled, if they are (spec 3.3).
    fn blocked(&self) -> Option<Blocked> {
        if let Some(b) = &self.busy {
            return Some(b.clone());
        }
        let l = self.loaded.as_ref()?;
        if l.live {
            return Some(Blocked::Live);
        }
        if let Err(reason) = &l.settings {
            return Some(Blocked::Settings(reason.clone()));
        }
        None
    }

    fn close_popup(&self) -> Task<Message> {
        match self.popup {
            Some(id) => cosmic::task::message(cosmic::Action::Surface(destroy_popup(id))),
            None => Task::none(),
        }
    }

    fn reload(&mut self) {
        self.loaded = Some(actions::load(&self.ctx));
    }

    /// Start the window, then close the popup, or show why it did not start.
    fn start_irida(&mut self, token: Option<String>) -> Task<Message> {
        self.opening = false;
        match actions::open_irida(&self.ctx, token) {
            Ok(()) => self.close_popup(),
            Err(reason) => {
                self.status = Some(reason);
                Task::none()
            }
        }
    }
}

impl Application for Window {
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
        let window = Window {
            core,
            ctx,
            popup: None,
            loaded: None,
            busy: None,
            status: None,
            token_tx: None,
            opening: false,
        };
        (window, Task::none())
    }

    fn on_close_requested(&self, id: Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    fn subscription(&self) -> Subscription<Message> {
        activation_token_subscription(0).map(|update| match update {
            TokenUpdate::Init(tx) => Message::TokenInit(TokenTx(tx)),
            TokenUpdate::ActivationToken { token, .. } => Message::TokenReady(token),
            TokenUpdate::Finished => Message::TokenGone,
        })
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::PopupClosed(id) => {
                if self.popup == Some(id) {
                    self.popup = None;
                    self.status = None;
                    self.opening = false;
                }
                Task::none()
            }
            Message::Apply(i) => {
                if self.blocked().is_some() {
                    return Task::none();
                }
                let Some(l) = &self.loaded else {
                    return Task::none();
                };
                let Ok(settings) = l.settings.clone() else {
                    return Task::none();
                };
                let Some(row) = self.rows().get(i).copied() else {
                    return Task::none();
                };
                let kind = row.kind.clone();
                let mode = l.mode;
                let ctx = self.ctx.clone();
                self.busy = Some(Blocked::Applying(kind.label()));
                self.status = None;
                cosmic::task::future(async move {
                    let result = tokio::task::spawn_blocking(move || {
                        actions::apply_row(&ctx, &settings, &kind, mode)
                    })
                    .await
                    .unwrap_or_else(|e| Err(e.to_string()));
                    Message::Applied(result)
                })
            }
            Message::Applied(result) => {
                self.busy = None;
                self.status = Some(match result {
                    Ok(text) | Err(text) => text,
                });
                self.reload();
                Task::none()
            }
            Message::Undo => {
                if self.blocked().is_some() {
                    return Task::none();
                }
                let ctx = self.ctx.clone();
                self.busy = Some(Blocked::Undoing);
                self.status = None;
                cosmic::task::future(async move {
                    let text = tokio::task::spawn_blocking(move || actions::undo(&ctx))
                        .await
                        .unwrap_or_else(|e| copy::undo_failed(&e.to_string()));
                    Message::Undone(text)
                })
            }
            Message::Undone(text) => {
                self.busy = None;
                self.status = Some(text);
                self.reload();
                Task::none()
            }
            Message::Open => {
                let Some(tx) = &self.token_tx else {
                    return self.start_irida(None);
                };
                let request = TokenRequest {
                    app_id: actions::IRIDA_APP_ID.to_string(),
                    exec: "irida".to_string(),
                };
                if tx.0.send(request).is_err() {
                    return self.start_irida(None);
                }
                self.opening = true;
                cosmic::task::future(async {
                    tokio::time::sleep(TOKEN_WAIT).await;
                    Message::OpenTimeout
                })
            }
            Message::TokenReady(token) => {
                if self.opening {
                    self.start_irida(token)
                } else {
                    Task::none()
                }
            }
            Message::OpenTimeout => {
                if self.opening {
                    self.start_irida(None)
                } else {
                    Task::none()
                }
            }
            Message::TokenInit(tx) => {
                self.token_tx = Some(tx);
                Task::none()
            }
            Message::TokenGone => {
                self.token_tx = None;
                Task::none()
            }
            Message::Close => match actions::close_irida() {
                Ok(()) => self.close_popup(),
                Err(reason) => {
                    self.status = Some(reason);
                    Task::none()
                }
            },
            Message::Remove => {
                let name = std::env::var("COSMIC_PANEL_NAME").ok();
                match panel::remove_self(name.as_deref()) {
                    Ok(()) => self.close_popup(),
                    Err(reason) => {
                        self.status = Some(reason);
                        Task::none()
                    }
                }
            }
            Message::Surface(a) => cosmic::task::message(cosmic::Action::Surface(a)),
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let have_popup = self.popup;
        let button =
            self.core
                .applet
                .icon_button(ICON)
                .on_press_with_rectangle(move |offset, bounds| {
                    if let Some(id) = have_popup {
                        Message::Surface(destroy_popup(id))
                    } else {
                        Message::Surface(app_popup::<Window>(
                            |_| Default::default(),
                            move |state: &mut Window| {
                                let new_id = Id::unique();
                                state.popup = Some(new_id);
                                state.status = None;
                                state.reload();
                                let mut settings = state.core.applet.get_popup_settings(
                                    state.core.main_window_id().unwrap_or(Id::RESERVED),
                                    new_id,
                                    None,
                                    None,
                                    None,
                                );
                                settings.positioner.anchor_rect = Rectangle {
                                    x: (bounds.x - offset.x) as i32,
                                    y: (bounds.y - offset.y) as i32,
                                    width: bounds.width as i32,
                                    height: bounds.height as i32,
                                };
                                settings
                            },
                            Some(Box::new(move |state: &Window| {
                                let body = popup_body(
                                    state.loaded.as_ref(),
                                    state.blocked().as_ref(),
                                    state.status.as_deref(),
                                );
                                Element::from(state.core.applet.popup_container(body))
                                    .map(cosmic::Action::App)
                            })),
                        ))
                    }
                });
        Element::from(self.core.applet.applet_tooltip::<Message>(
            button,
            "Irida",
            self.popup.is_some(),
            Message::Surface,
            None,
        ))
    }

    fn view_window(&self, _id: Id) -> Element<'_, Message> {
        widget::Space::new().into()
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        Some(cosmic::applet::style())
    }
}

fn heading(label: &'static str) -> Element<'static, Message> {
    cosmic::applet::padded_control(text::caption(label)).into()
}

/// A palette row: its label, the tick when it was applied last, and a click
/// that applies it unless the popup is blocked.
fn row(index: usize, row: &Row, enabled: bool) -> Element<'static, Message> {
    let mut content = widget::Row::new()
        .align_y(Alignment::Center)
        .push(text::body(row.kind.label()).width(Length::Fill));
    if row.ticked {
        content = content.push(widget::icon::from_name(TICK).size(16).symbolic(true).icon());
    }
    let button = cosmic::applet::menu_button(content);
    if enabled {
        button.on_press(Message::Apply(index)).into()
    } else {
        button.into()
    }
}

fn action(label: &'static str, message: Option<Message>) -> Element<'static, Message> {
    let button = cosmic::applet::menu_button(text::body(label));
    match message {
        Some(m) => button.on_press(m).into(),
        None => button.into(),
    }
}

/// The popup's content (spec 3.2): the sections, the greyed line, the
/// divider, the actions and the status line. Free of the window so a
/// headless test can draw it.
pub fn popup_body(
    loaded: Option<&Loaded>,
    blocked: Option<&Blocked>,
    status: Option<&str>,
) -> Element<'static, Message> {
    let enabled = blocked.is_none();
    let mut items: Vec<Element<'static, Message>> = Vec::new();
    let mut index = 0;
    let mut section = |items: &mut Vec<Element<'static, Message>>, label, rows: &[Row]| {
        if rows.is_empty() {
            return;
        }
        items.push(heading(label));
        for r in rows {
            items.push(row(index, r, enabled));
            index += 1;
        }
    };
    let (empty, window_open) = match loaded {
        Some(l) => {
            section(&mut items, copy::HEADING_PAIRS, &l.menu.pairs);
            section(&mut items, copy::HEADING_DARK, &l.menu.dark);
            section(&mut items, copy::HEADING_LIGHT, &l.menu.light);
            (l.menu.is_empty(), l.window_open)
        }
        None => (true, false),
    };
    if empty {
        items.push(cosmic::applet::padded_control(text::body(copy::NO_PALETTES)).into());
    }
    if let Some(b) = blocked {
        items.push(cosmic::applet::padded_control(text::caption(b.text())).into());
    }
    items.push(
        cosmic::applet::padded_control(widget::divider::horizontal::default())
            .padding([8, 0])
            .into(),
    );
    items.push(action(copy::UNDO, enabled.then_some(Message::Undo)));
    items.push(action(copy::OPEN, Some(Message::Open)));
    if window_open {
        items.push(action(copy::CLOSE, Some(Message::Close)));
    }
    items.push(action(copy::REMOVE, Some(Message::Remove)));
    if let Some(s) = status {
        items.push(cosmic::applet::padded_control(text::caption(s.to_string())).into());
    }
    widget::Column::with_children(items)
        .width(Length::Fill)
        .into()
}
