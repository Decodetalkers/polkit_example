use iced::Element;
use iced::Task;
use iced::widget::button;
use iced::window::Id;
use iced::{
    Alignment, Length,
    widget::{column, container, text, text_input},
};
use iced_layershell::settings::LayerShellSettings;
use iced_layershell::to_layer_message;
use iced_layershell::{
    daemon,
    reexport::NewLayerShellSettings,
    reexport::{Anchor, Layer},
};

#[to_layer_message(multi)]
#[derive(Debug, Clone)]
enum Message {
    PolkitCome,
    PolkitError(String),
    PolkitInfo(String),
    Cancel(Id),
    PassworkChange(String),
}

#[derive(Debug)]
struct PolkitDialog {
    error_message: String,
    info_message: String,
    password: String,
}

const DIALOG_NAMESPACE: &str = "polkit";

impl PolkitDialog {
    fn new() -> Self {
        Self {
            error_message: "".to_owned(),
            info_message: "".to_owned(),
            password: "".to_owned(),
        }
    }
}

fn update(dialog: &mut PolkitDialog, message: Message) -> Task<Message> {
    //use iced::window::Action as WindowAction;
    use iced_runtime::Action;
    match message {
        Message::PolkitCome => Task::done(Message::NewLayerShell {
            settings: NewLayerShellSettings {
                size: None,
                exclusive_zone: Some(-1),
                anchor: Anchor::all(),
                layer: Layer::Top,
                use_last_output: false,
                ..Default::default()
            },
            id: iced::window::Id::unique(),
        }),
        Message::Cancel(_id) => iced_runtime::task::effect(Action::Exit),
        Message::PolkitError(err) => {
            dialog.error_message = err;
            Task::none()
        }
        Message::PolkitInfo(info) => {
            dialog.info_message = info;
            Task::none()
        }
        Message::PassworkChange(password) => {
            dialog.password = password;
            Task::none()
        }
        _ => Task::none(),
    }
}

fn view(dialog: &PolkitDialog, id: iced::window::Id) -> Element<Message> {
    container(
        column![
            text_input("Your password", &dialog.password)
                .padding(10)
                .secure(true)
                .on_input(Message::PassworkChange),
            text("info"),
            button("Confirm").on_press(Message::Cancel(id))
        ]
        .align_x(Alignment::Center)
        .width(Length::Fixed(700.)),
    )
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn style(_dialog: &PolkitDialog, theme: &iced::Theme) -> iced::theme::Style {
    use iced::theme::Style;
    Style {
        background_color: iced::Color::from_rgba(0.2, 0.2, 0.2, 0.8),
        text_color: theme.palette().text,
    }
}

fn main() -> iced_layershell::Result {
    daemon(PolkitDialog::new, DIALOG_NAMESPACE, update, view)
        .layer_settings(LayerShellSettings {
            exclusive_zone: -1,
            anchor: Anchor::all(),
            ..Default::default()
        })
        .style(style)
        .run()
}
