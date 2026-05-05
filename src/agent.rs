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

async fn authenticate(
    agent: &mut Agent,
    _action_id: &str,
    msg: &str,
    _icon_name: &str,
    _details: HashMap<&str, &str>,
    cookie: &str,
    mut identifiers: Vec<Identity<'_>>,
) -> Result<(), Error> {
    let identify: UnixUser = identifiers.remove(0).try_into()?;
    let mut session = PolkitAgentSession::new(identify, cookie)?;
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
                        Err(e) => {
                            let _ = agent.sender.send(Message::PolkitComplete).await;
                            return Err(Error::FailedReason(format!("Time out: {e}")));
                        }
                    }
                }
                _ => {}
            }
        }

        if session.succeeded() {
            let _ = agent.sender.send(Message::PolkitComplete).await;
            return Ok(());
        }
        session.restart()?;
        retry_count -= 1;
    }
    let _ = agent.sender.send(Message::PolkitComplete).await;
    if !session.succeeded() {
        return Err(Error::Failed);
    }
    Ok(())
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
            sender: sender.clone(),
        },
        authenticate,
        cancel_authentication,
    )
    .connect(OBJECT_PATH)
    .await
}
