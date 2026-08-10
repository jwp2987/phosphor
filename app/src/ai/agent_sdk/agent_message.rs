//! `oz agent message send`/`list` CLI dispatch.
//!
//! Thin formatting layer over `warp_cli::agent_mailbox`'s pure filesystem
//! mailbox. Deliberately synchronous and independent of `AppContext` state --
//! unlike `oz agent run`, sending or listing mail needs no BYOP provider, no
//! signed-in state, and no running app instance; it is the same on-disk
//! mailbox `SendMessageToAgentExecutor` (`app/src/ai/blocklist/action_model/execute/send_message.rs`)
//! writes into directly when a Zap-native agent conversation's model invokes
//! the `SendMessageToAgent` tool.
use comfy_table::Cell;
use warp_cli::agent::{
    AgentMessageCommand, AgentMessageListArgs, AgentMessageSendArgs, OutputFormat,
};
use warp_cli::agent_mailbox::{self, MailboxMessage};

use super::output::{self, TableFormat};

pub fn run(output_format: OutputFormat, command: AgentMessageCommand) -> anyhow::Result<()> {
    match command {
        AgentMessageCommand::Send(args) => send(output_format, args),
        AgentMessageCommand::List(args) => list(output_format, args),
    }
}

fn send(output_format: OutputFormat, args: AgentMessageSendArgs) -> anyhow::Result<()> {
    let root = agent_mailbox::mailbox_root();
    let mut sent = Vec::with_capacity(args.to.len());
    for to in &args.to {
        sent.push(agent_mailbox::send_message(
            &root,
            &args.sender_run_id,
            to,
            &args.subject,
            &args.body,
        )?);
    }

    match output_format {
        OutputFormat::Json => output::write_json(&sent, std::io::stdout())?,
        OutputFormat::Ndjson => {
            for message in &sent {
                output::write_json_line(message, std::io::stdout())?;
            }
        }
        OutputFormat::Pretty | OutputFormat::Text => {
            for message in &sent {
                println!("Sent message {} to {}", message.message_id, message.to);
            }
        }
    }
    Ok(())
}

fn list(output_format: OutputFormat, args: AgentMessageListArgs) -> anyhow::Result<()> {
    let root = agent_mailbox::mailbox_root();
    let messages = agent_mailbox::list_messages(&root, &args.run_id, args.limit as usize)?;
    output::print_list(messages, output_format);
    Ok(())
}

impl TableFormat for MailboxMessage {
    fn header() -> Vec<Cell> {
        vec![
            Cell::new("Message ID"),
            Cell::new("From"),
            Cell::new("Subject"),
            Cell::new("Body"),
            Cell::new("Sent At"),
        ]
    }

    fn row(&self) -> Vec<Cell> {
        vec![
            Cell::new(&self.message_id),
            Cell::new(&self.from),
            Cell::new(&self.subject),
            Cell::new(&self.body),
            Cell::new(self.sent_at.to_rfc3339()),
        ]
    }
}
