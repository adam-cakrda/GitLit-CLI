use GitLit_CLI::GitLitClient;
use clap::{Parser, Subcommand};
use serde_json::json;

#[derive(Parser)]
#[command(name = "gitlit")]
#[command(about = "GitLit API CLI", long_about = None)]
pub struct Cli {
    #[arg(long, env = "GITLIT_URL")]
    pub url: String,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    Login {
        #[arg(long, env = "GITLIT_LOGIN")] login: String,
        #[arg(long, env = "GITLIT_PASSWORD")] password: String,
    },
    Register {
        #[arg(long)] username: String,
        #[arg(long)] email: String,
        #[arg(long)] password: String,
    },
    Logout,
    Repos {
        #[arg(long)] owner: Option<String>,
        #[arg(long)] filter: Option<String>,
        #[arg(long)] q: Option<String>,
    },
    CreateRepo {
        #[arg(long)] name: String,
        #[arg(long)] description: Option<String>,
        #[arg(long)] private: Option<bool>,
    },
    DeleteRepo { #[arg(long)] id: String },
    Branches { #[arg(long)] id: String },
    Commits { #[arg(long)] id: String, #[arg(long)] branch: Option<String>, #[arg(long)] limit: Option<u32> },
    Content { #[arg(long)] id: String, #[arg(long)] path: Option<String>, #[arg(long)] branch: Option<String>, #[arg(long)] commit: Option<String> },
    Download { #[arg(long)] id: String, #[arg(long)] path: Option<String>, #[arg(long)] branch: Option<String>, #[arg(long)] commit: Option<String>, #[arg(long)] out: String },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let url = if cli.url.starts_with("http://") || cli.url.starts_with("https://") {
        cli.url.clone()
    } else {
        format!("https://{}", cli.url)
    };

    let client = GitLitClient::new(&url)?;
    match cli.command {
        Commands::Login { login, password } => {
            let token = client.login(&login, &password).await?;
            println!("{}", serde_json::to_string_pretty(&json!({
                "status": "ok",
                "token": token
            }))?);
        }
        Commands::Logout => {
            match client.logout().await {
                Ok(()) => println!("{}", serde_json::to_string_pretty(&json!({"status":"ok"}))?),
                Err(e) => {
                    println!("{}", serde_json::to_string_pretty(&json!({
                        "status": "error",
                        "error": e.to_string()
                    }))?);
                }
            }
        }
        Commands::Register { username, email, password } => {
            let res = client.register(&username, &email, &password).await?;
            println!("{}", serde_json::to_string_pretty(&json!({"status":"ok","response": res}))?);
        }
        Commands::Repos { owner, filter, q } => {
            let repos = client.list_repos(owner.as_deref(), filter.as_deref(), q.as_deref()).await?;
            println!("{}", serde_json::to_string_pretty(&json!({"status":"ok","repos": repos}))?);
        }
        Commands::CreateRepo { name, description, private } => {
            let repo = client.create_repo(&name, description.as_deref(), private).await?;
            println!("{}", serde_json::to_string_pretty(&json!({"status":"ok","repo": repo}))?);
        }
        Commands::DeleteRepo { id } => {
            match client.delete_repo(&id).await {
                Ok(ok) => println!("{}", serde_json::to_string_pretty(&json!({"status":"ok","ok": ok.ok}))?),
                Err(e) => println!("{}", serde_json::to_string_pretty(&json!({"status":"error","error": e.to_string()}))?),
            }
        }
        Commands::Branches { id } => {
            let br = client.branches(&id).await?;
            println!("{}", serde_json::to_string_pretty(&json!({"status":"ok","branches": br.branches}))?);
        }
        Commands::Commits { id, branch, limit } => {
            let commits = client.commits(&id, branch.as_deref(), limit).await?;
            println!("{}", serde_json::to_string_pretty(&json!({"status":"ok","commits": commits}))?);
        }
        Commands::Content { id, path, branch, commit } => {
            let content = client.content(&id, path.as_deref(), branch.as_deref(), commit.as_deref()).await?;
            println!("{}", serde_json::to_string_pretty(&json!({"status":"ok","content": content}))?);
        }
        Commands::Download { id, path, branch, commit, out } => {
            let bytes = client.download(&id, path.as_deref(), branch.as_deref(), commit.as_deref()).await?;
            std::fs::write(&out, &bytes)?;
            println!("{}", serde_json::to_string_pretty(&json!({"status":"ok","bytes": bytes.len()}))?);
        }
    }
    Ok(())
}
