//! General-purpose administrative commands in the Zap CLI.

use std::io::Write;

use anyhow::{Context, Result};
use serde::Serialize;
use warp_cli::agent::OutputFormat;
use warpui::{platform::TerminationMode, AppContext, SingletonEntity};

/// What `whoami` reports.
///
/// Phosphor has no accounts: every run is a local profile using the user's own provider
/// keys. `whoami` used to print the auth facade's placeholder identity
/// (`test_user_uid` / `test_user@warp.dev`, `crate::auth::TEST_USER_*`) as if it were a
/// signed-in user (#637). It now says what is actually true.
#[derive(Serialize)]
struct WhoamiOutput {
    /// Always `"local"`.
    #[serde(rename = "type")]
    principal_type: &'static str,
    /// Always `null`: there is no account to report.
    account: Option<String>,
}

const LOCAL_WHOAMI: WhoamiOutput = WhoamiOutput {
    principal_type: "local",
    account: None,
};

/// Singleton model that provides a `ModelContext` for the `whoami` command's async work.
struct WhoamiRunner;

impl warpui::Entity for WhoamiRunner {
    type Event = ();
}

impl SingletonEntity for WhoamiRunner {}

/// Write the `whoami` report in `output_format`.
fn write_whoami<W: Write>(output_format: OutputFormat, w: &mut W) -> Result<()> {
    let info = &LOCAL_WHOAMI;
    match output_format {
        OutputFormat::Json => {
            serde_json::to_writer(&mut *w, info).context("whoami output should serialize")?;
            writeln!(w)?;
        }
        OutputFormat::Pretty => {
            writeln!(w, "Local profile (no account)")?;
            writeln!(
                w,
                "Phosphor has no sign-in: agents run locally with the model provider keys you configure."
            )?;
        }
        OutputFormat::Text => {
            writeln!(w, "{}", info.principal_type)?;
        }
        OutputFormat::Ndjson => {
            anyhow::bail!("`whoami` does not support `--output-format ndjson`");
        }
    }
    Ok(())
}

/// Print who the CLI runs as: always a local profile, never an account.
pub fn whoami(ctx: &mut AppContext, output_format: OutputFormat) -> Result<()> {
    // Terminate from a spawned callback, as the other CLI commands do, so the app is
    // fully running when it is asked to exit.
    let runner = ctx.add_singleton_model(|_| WhoamiRunner);
    runner.update(ctx, move |_, ctx| {
        ctx.spawn(futures::future::ready(()), move |_, _, ctx| {
            let result = write_whoami(output_format, &mut std::io::stdout().lock());
            ctx.terminate_app(TerminationMode::ForceTerminate, result.err().map(Err));
        });
    });

    Ok(())
}

#[cfg(test)]
#[path = "admin_tests.rs"]
mod tests;
