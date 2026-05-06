use zbus_polkit_agent::{
    Identity, UnixUser,
    agent_session::{Message as PolkitMessage, PolkitAgentSession, Response},
    polkit_agent_instance,
    reexport::Connection,
    server::Error,
};

use std::collections::HashMap;

use futures::channel::mpsc::{Receiver, Sender};

use futures::SinkExt;

use crate::{Message, PasswordMessage};

const OBJECT_PATH: &str = "/org/waycrate/PolicyKit1/AuthenticationAgent";

struct Agent {
    pw_receiver: Receiver<PasswordMessage>,
    sender: Sender<Message>,
}

async fn authenticate_inner(
    agent: &mut Agent,
    action_id: &str,
    msg: &str,
    icon_name: &str,
    _details: HashMap<&str, &str>,
    cookie: &str,
    identifiers: Vec<Identity<'_>>,
) -> Result<(), Error> {
    let users: Vec<UnixUser> = identifiers
        .iter()
        .flat_map(|identify| identify.try_into())
        .collect();
    let user_names: Vec<String> = users
        .iter()
        .map(|user| nix::unistd::User::from_uid(user.uid.into()))
        .map(|result| {
            result
                .map(|user| user.map(|user| user.name).unwrap_or("Unknown".to_owned()))
                .unwrap_or("Unknown".to_owned())
        })
        .collect();
    let _ = agent
        .sender
        .send(Message::PolkitComing {
            user_names,
            action_id: action_id.to_owned(),
            icon_name: icon_name.to_owned(),
        })
        .await;
    let mut session = PolkitAgentSession::new(users[0], cookie)?;
    let mut retry_count = 3;
    while retry_count >= 0 {
        while !session.is_complete() {
            let message = session.async_dispatch().await?;
            match message {
                PolkitMessage::Error(error) => {
                    tracing::error!("error: {error}");
                    let _ = agent.sender.send(Message::PolkitError(error)).await;
                }
                PolkitMessage::Info(info) => {
                    let _ = agent.sender.send(Message::PolkitInfo(info)).await;
                }
                PolkitMessage::Request { prompt, .. } => {
                    let _ = agent
                        .sender
                        .send(Message::PolkitPasswordRequest {
                            prompt,
                            message: msg.to_owned(),
                        })
                        .await;
                    match agent.pw_receiver.recv().await {
                        Ok(PasswordMessage::Password(password)) => {
                            session.response(Response {
                                password: &password,
                            })?;
                        }
                        Ok(PasswordMessage::Canceled) => {
                            return Err(Error::Cancelled);
                        }
                        Ok(PasswordMessage::SwitchUser(index)) => {
                            session.restart_with_uid(users[index])?;
                        }
                        Err(e) => {
                            return Err(Error::FailedReason(format!("Timeout: {e}")));
                        }
                    }
                }
                _ => {}
            }
        }

        if session.succeeded() {
            return Ok(());
        }
        session.restart()?;
        retry_count -= 1;
    }
    if !session.succeeded() {
        return Err(Error::Failed);
    }
    Ok(())
}

async fn authenticate(
    agent: &mut Agent,
    action_id: &str,
    msg: &str,
    icon_name: &str,
    details: HashMap<&str, &str>,
    cookie: &str,
    identifiers: Vec<Identity<'_>>,
) -> Result<(), Error> {
    let result = authenticate_inner(
        agent,
        action_id,
        msg,
        icon_name,
        details,
        cookie,
        identifiers,
    )
    .await;
    let _ = agent.sender.send(Message::PolkitComplete).await;
    result
}

async fn cancel_authentication(agent: &mut Agent, _cookie: &str) -> Result<(), Error> {
    let _ = agent.sender.send(Message::PolkitComplete).await;
    Ok(())
}

pub async fn init_agent(
    pw_receiver: Receiver<PasswordMessage>,
    sender: Sender<Message>,
) -> Result<Connection, zbus_polkit_agent::error::Error> {
    polkit_agent_instance(
        move || Agent {
            pw_receiver,
            sender,
        },
        authenticate,
        cancel_authentication,
    )
    .connect(OBJECT_PATH)
    .await
}
