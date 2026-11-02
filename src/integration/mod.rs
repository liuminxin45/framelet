//! User-scoped integration transactions. No capture, daemon, download or shell is started.
mod config;
mod probe;
#[cfg(test)]
mod tests;
mod transaction;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Client {
    Codex,
    ClaudeCode,
    Cursor,
    ClaudeDesktop,
}
impl Client {
    fn name(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::ClaudeCode => "claude-code",
            Self::Cursor => "cursor",
            Self::ClaudeDesktop => "claude-desktop",
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Action {
    Install,
    Check,
    Uninstall,
}
#[derive(clap::Args)]
pub struct Args {
    #[arg(value_enum)]
    pub action: Action,
    #[arg(long, value_enum)]
    pub client: Client,
    /// Explicit config for a custom profile; defaults to the client's user config.
    #[arg(long)]
    pub config: Option<PathBuf>,
    /// Explicit directory for the Framelet skill, for a custom profile.
    #[arg(long)]
    pub skill_dir: Option<PathBuf>,
    /// Recording root; defaults to LOCALAPPDATA/Framelet/Captures.
    #[arg(long)]
    pub output_root: Option<PathBuf>,
    /// Explicitly take over an existing server entry; uninstall restores it.
    #[arg(long)]
    pub adopt_existing: bool,
}
pub(super) struct Layout {
    pub client: Client,
    pub config: PathBuf,
    pub skill: Option<PathBuf>,
    pub state: PathBuf,
    pub journal: PathBuf,
    pub backup: PathBuf,
}
impl Layout {
    fn new(a: &Args) -> Result<Self> {
        let home = std::env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .context("USERPROFILE unavailable")?;
        let config = match &a.config {
            Some(p) => p.clone(),
            None => match a.client {
                Client::Codex => std::env::var_os("CODEX_HOME")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| home.join(".codex"))
                    .join("config.toml"),
                Client::ClaudeCode => std::env::var_os("CLAUDE_CONFIG_DIR")
                    .map(PathBuf::from)
                    .map_or_else(|| home.join(".claude.json"), |p| p.join(".claude.json")),
                Client::Cursor => home.join(".cursor/mcp.json"),
                Client::ClaudeDesktop => {
                    PathBuf::from(std::env::var_os("APPDATA").context("APPDATA unavailable")?)
                        .join("Claude/claude_desktop_config.json")
                }
            },
        };
        let config = local(&config)?;
        let default_skill = if a.client == Client::ClaudeCode {
            std::env::var_os("CLAUDE_CONFIG_DIR")
                .map(PathBuf::from)
                .map(|p| p.join("skills/framelet"))
        } else {
            None
        };
        let skill = if a.client == Client::ClaudeDesktop {
            ensure!(
                a.skill_dir.is_none(),
                "Claude Desktop skills are installed through its supported UI/plugin flow; --skill-dir is not supported"
            );
            None
        } else {
            Some(local(
                &a.skill_dir.clone().or(default_skill).unwrap_or_else(|| {
                    home.join(match a.client {
                        Client::Codex => ".agents/skills/framelet",
                        Client::ClaudeCode => ".claude/skills/framelet",
                        Client::Cursor => ".cursor/skills/framelet",
                        Client::ClaudeDesktop => unreachable!(),
                    })
                }),
            )?)
        };
        let parent = config.parent().context("config parent missing")?;
        let key = hash(config.to_string_lossy().to_lowercase().as_bytes())?;
        let base = parent.join(".framelet-integrations");
        Ok(Self {
            client: a.client,
            config,
            skill,
            state: base.join(format!("{}-{}.json", a.client.name(), &key[..16])),
            journal: base.join(format!("{}-{}.pending.json", a.client.name(), &key[..16])),
            backup: base.join("backups"),
        })
    }
}
fn local(path: &Path) -> Result<PathBuf> {
    let path = std::path::absolute(path)?;
    ensure!(
        matches!(path.components().next(),Some(std::path::Component::Prefix(p))if matches!(p.kind(),std::path::Prefix::Disk(_)|std::path::Prefix::VerbatimDisk(_))),
        "integration paths require a local Windows drive"
    );
    ensure!(
        path.file_name()
            .is_some_and(|n| !n.to_string_lossy().contains(':')),
        "invalid integration filename"
    );
    ensure!(path.to_str().is_some(), "integration path must be Unicode");
    Ok(path)
}
pub(super) fn hash(bytes: &[u8]) -> Result<String> {
    crate::identity::sha256_reader(std::io::Cursor::new(bytes))
}
pub(super) fn read_optional(path: &Path, limit: u64) -> Result<Option<Vec<u8>>> {
    if !path.try_exists()? {
        return Ok(None);
    };
    let lease = crate::file_guard::Lease::input(path)?;
    ensure!(
        path.metadata()?.len() <= limit,
        "integration file exceeds byte budget"
    );
    use std::io::Read;
    let mut data = Vec::new();
    lease.reader()?.take(limit + 1).read_to_end(&mut data)?;
    ensure!(
        data.len() as u64 <= limit,
        "integration file exceeds byte budget"
    );
    Ok(Some(data))
}
fn files(exe: &Path, client: Client) -> Result<BTreeMap<String, String>> {
    let mut files = BTreeMap::from([
        (
            "SKILL.md".into(),
            include_str!("../../skills/framelet/SKILL.md").into(),
        ),
        (
            "references/testing.md".into(),
            include_str!("../../skills/framelet/references/testing.md").into(),
        ),
        (
            "references/demos.md".into(),
            include_str!("../../skills/framelet/references/demos.md").into(),
        ),
        (
            "references/editing.md".into(),
            include_str!("../../skills/framelet/references/editing.md").into(),
        ),
        (
            "agents/openai.yaml".into(),
            include_str!("../../skills/framelet/agents/openai.yaml").into(),
        ),
    ]);
    files.insert("references/installation.json".into(),serde_json::to_string_pretty(&json!({"executable":exe,"client":client,"version":env!("CARGO_PKG_VERSION"),"transport":"stdio","args":["mcp"]}))?);
    Ok(files)
}
fn entry(a: &Args) -> Result<Value> {
    let exe = std::env::current_exe()?;
    let output = local(&a.output_root.clone().unwrap_or_else(|| {
        PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap_or_else(|| ".".into()))
            .join("Framelet/Captures")
    }))?;
    let mut value = json!({"command":exe,"args":["mcp"],"env":{"FRAMELET_MCP_ROOT":output,"FRAMELET_MCP_READ_ROOTS":"[]"}});
    if a.client == Client::Codex {
        value["tool_timeout_sec"] = json!(150);
    } else if a.client == Client::ClaudeCode {
        value["type"] = json!("stdio");
    }
    Ok(value)
}
pub fn run(a: &Args) -> Result<Value> {
    ensure!(
        a.action == Action::Install || (!a.adopt_existing && a.output_root.is_none()),
        "adopt-existing/output-root apply only to install"
    );
    let layout = Layout::new(a)?;
    if a.action != Action::Check {
        std::fs::create_dir_all(layout.state.parent().context("state parent")?)?;
    }
    let _lock = transaction::Lock::acquire(&layout.config)?;
    let _skill_lock = layout
        .skill
        .as_ref()
        .map(|p| transaction::Lock::acquire(p))
        .transpose()?;
    if a.action == Action::Check {
        return check(&layout);
    }
    transaction::recover(&layout)?;
    let mut old = transaction::receipt(&layout)?;
    let cfg = config::Config::load(&layout.config, a.client)?;
    let current = cfg.entry()?;
    match a.action {
        Action::Install => {
            let desired = entry(a)?;
            if let Some(existing) = &old {
                ensure!(
                    current == existing.entry,
                    "managed server entry changed; preserve edits and inspect the receipt before retrying"
                );
            } else {
                ensure!(
                    current.is_none() || a.adopt_existing,
                    "server entry exists but is unmanaged; use --adopt-existing explicitly to preserve and take over it"
                );
            }
            let original = old
                .as_ref()
                .map(|r| r.original.clone())
                .unwrap_or_else(|| current.clone());
            let target = transaction::Receipt {
                schema: 1,
                client: a.client,
                config: layout.config.clone(),
                skill: layout.skill.clone(),
                entry: Some(desired.clone()),
                original,
                files: if layout.skill.is_some() {
                    files(&std::env::current_exe()?, a.client)?
                } else {
                    BTreeMap::new()
                },
            };
            transaction::validate_files(&layout, old.as_ref(), Some(&target))?;
            // A failed executable/read-root handshake never rewrites the client's config.
            probe::run(&desired)?;
            transaction::begin(&layout, old.take(), Some(target), current)?;
            transaction::recover(&layout)?;
            let mut value = check(&layout).with_context(|| {
                format!(
                    "integration is installed at {}; final check failed; retry integrate check",
                    layout.config.display()
                )
            })?;
            value["action"] = json!("installed");
            Ok(value)
        }
        Action::Uninstall => {
            let Some(old) = old else {
                return Ok(
                    json!({"action":"not_installed","client":a.client,"config":layout.config,"recordings_preserved":true}),
                );
            };
            ensure!(
                current == old.entry,
                "managed server entry changed; refusing to remove user edits"
            );
            transaction::validate_files(&layout, Some(&old), None)?;
            transaction::begin(&layout, Some(old), None, current)?;
            transaction::recover(&layout)?;
            Ok(
                json!({"action":"uninstalled","client":a.client,"config":layout.config,"recordings_preserved":true,"restored_preexisting_entry":config::Config::load(&layout.config,a.client)?.entry()?.is_some(),"binary_removed":false,"running_clients_terminated":false}),
            )
        }
        Action::Check => unreachable!(),
    }
}
fn check(layout: &Layout) -> Result<Value> {
    ensure!(
        !layout.journal.try_exists()?,
        "integration transaction interrupted; retry install/uninstall to finish it"
    );
    let receipt = transaction::receipt(layout)?;
    let current = config::Config::load(&layout.config, layout.client)?
        .entry()?
        .context("Framelet server is not configured")?;
    let managed = receipt.is_some();
    if let Some(receipt) = &receipt {
        ensure!(
            receipt.entry.as_ref() == Some(&current),
            "server entry differs from managed receipt"
        );
        transaction::validate_files(layout, Some(receipt), Some(receipt))?;
    }
    let probe = probe::run(&current)?;
    Ok(
        json!({"client":layout.client,"config":layout.config,"managed":managed,"skill":if managed{layout.skill.clone()}else{None},"skill_registration":if layout.client==Client::ClaudeDesktop{"manual client skill/plugin installation"}else{"filesystem discovery; reconnect client"},"handshake":probe,"requires_reconnect":true,"client_application_tested":false,"recordings_preserved":true,"state":layout.state}),
    )
}
