use futures::stream::StreamExt;
use gitdown::error::{Error, ErrorKind, Result};
use log::error;
use reqwest::StatusCode;
use reqwest::{Client as ReqwestClient, RequestBuilder, Response};
use serde::Deserialize;
use std::fmt::Display;
use std::io;
use std::io::prelude::*;
use std::process::{Command, Stdio};

/// A GitHub directory entry.
///
///
#[derive(Debug, Deserialize, Clone)]
pub struct GitHubDirEntry {
    /// The path in the repository (not to be confused with the url)
    path: Option<String>,
    /// The file type -- can be `blob` or `tree`.
    #[serde(rename = "type")]
    ty: String,
    /// The size of the entry.
    #[allow(dead_code)]
    size: Option<usize>,
    /// The raw githubusercontent url
    #[serde(skip_serializing)]
    raw_path: Option<String>,
}

pub struct Client<'a> {
    client: ReqwestClient,
    base_url: &'a str,
}

impl<'a> Client<'a> {
    pub fn from_url(base_url: &'a str) -> Result<Self> {
        let b = ReqwestClient::builder().user_agent("gitdown");

        Ok(Self {
            client: b.build()?,
            base_url,
        })
    }

    pub async fn send(&self, mut req: RequestBuilder) -> Result<Response> {
        req = req.header("Content-Type", "application/vnd.github.v3+json");

        let res = req.send().await?;
        let status = res.status();

        if status == StatusCode::OK {
            Ok(res)
        } else {
            Error::err(ErrorKind::GitHubStatusFailure {
                status,
                msg: res.text().await.unwrap(),
            })
        }
    }

    pub async fn get_dentries(
        &self,
        username: &str,
        repo: &str,
        tree: Option<&str>,
    ) -> Result<Vec<GitHubDirEntry>> {
        let tree = if let Some(t) = tree { t } else { "main" }.to_string();
        let mut query = format!("{}/{}/git/trees/{}", username, repo, tree);

        // This option recursively walks the tree of the repository,
        // yielding all blobs (and even trees).
        query.push_str("?recursive=1");

        let url = format!("{}/{}", self.base_url, query);
        let req = self.client.get(url.as_str());

        let res = if let Ok(r) = self.send(req).await {
            r
        } else {
            return Error::err(ErrorKind::TreeDoesNotExist {
                tree,
                repo: format!("{}/{}", username, repo),
            });
        };

        let text = res.text().await?;
        let body: serde_json::Value = serde_json::from_str(&text).unwrap();
        if let Some(dentries) = body.get("tree") {
            let dentries: Vec<GitHubDirEntry> =
                serde_json::from_value(dentries.to_owned()).unwrap();

            // Earlier, we yielded everything, but really we only want blobs.
            Ok(dentries.into_iter().filter(|d| d.ty == "blob").collect())
        } else {
            Error::err(ErrorKind::ResponseKeyError {
                key: "tree".to_string(),
            })
        }
    }
}

fn get_from_fzf<I, D>(items: I) -> Result<Option<Vec<String>>>
where
    I: IntoIterator<Item = D>,
    D: Display,
{
    let mut command = Command::new("fzf");
    command.stdin(Stdio::piped()).stdout(Stdio::piped());
    command.args(&[
        "-m",
        "--bind=ctrl-z:ignore",
        "--exit-0",
        "--height=40%",
        "--inline-info",
        "--no-sort",
        "--reverse",
        "--select-1",
    ]);

    let mut child = command.spawn()?;
    {
        // We require a new scope as `stdin` mutably borrows `child.stdin`, so
        // it must be dropped before `child.wait()`.
        let mut stdin = io::BufWriter::new(child.stdin.as_mut().unwrap());

        for item in items.into_iter() {
            writeln!(&mut stdin, "{}", item)?;
        }
    }

    let status = child.wait()?;

    if status.success() {
        let mut output = String::new();
        child.stdout.unwrap().read_to_string(&mut output)?;

        let vec = output
            .trim()
            .split("\n")
            .collect::<Vec<&str>>()
            .iter()
            .map(|&s| s.into())
            .collect();

        Ok(Some(vec))
    } else {
        // On Unix, the `status.code()` will be `None` if the process was
        // terminated by a signal. So the `gitdown` process was either killed
        // by a signal or a file wasn't chosen.
        match status.code() {
            None => Error::err(ErrorKind::Interrupted),
            Some(_) => Error::err(ErrorKind::Other {
                status: format!(
                    "An error occured; likely, a file was not chosen: {}",
                    status
                ),
            }),
        }
    }
}

use clap::arg;

fn parse_argv() -> Result<(String, String)> {
    let matches = clap::Command::new("gitdown")
        .author("steven-mathew")
        .version("v0.1.0")
        .about("Download specific files from a repository (taken from clipboard by default)")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .allow_external_subcommands(true)
        .allow_invalid_utf8_for_external_subcommands(true)
        .subcommand(
            clap::Command::new("repo")
                .about("Repository downloading from")
                .arg(arg!(<REPO> "The repo to download from"))
                .arg_required_else_help(true),
        )
        .get_matches();

    match matches.subcommand() {
        Some(("repo", sub_matches)) => {
            let text = if let Some(text) = sub_matches.value_of("REPO") {
                text.to_string()
            } else {
                // TODO: Add clipboard support
                // let mut ctx: ClipboardContext = ClipboardProvider::new()?;
                // ctx.get_contents()?
                return Error::err(ErrorKind::EmptyText);
            };

            if text.matches("/").count() != 1 {
                return Error::err(ErrorKind::MalformedRepo { repo: text });
            }

            let (user, repo) = text.split_once("/").unwrap();
            Ok((user.to_string(), repo.to_string()))
        }
        _ => {
            unimplemented!()
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let (user, repo) = parse_argv()?;
    let client = Client::from_url("https://api.github.com/repos");

    let res = client
        .expect("Could not establish a connection with the GitHub API.")
        .get_dentries(user.as_str(), repo.as_str(), None)
        .await?;

    let paths = res
        .into_iter()
        .map(|gh| gh.path.unwrap())
        .collect::<Vec<String>>();

    if let Some(paths) = get_from_fzf(paths).unwrap() {
        let mut urls: Vec<GitHubDirEntry> = paths
            .into_iter()
            .map(|path| {
                let raw_path = format!(
                    "https://raw.githubusercontent.com/{}/{}/main/{}",
                    user, repo, path
                );

                GitHubDirEntry {
                    path: Some(path),
                    ty: "blob".to_string(), // At this point, we can assume only blobs are given.
                    size: None,
                    raw_path: Some(raw_path),
                }
            })
            .collect();

        let client = ReqwestClient::builder().build()?;

        let fetches = futures::stream::iter(urls.drain(..).map(|dentry| {
            use std::fs;

            let raw_path = dentry.raw_path.unwrap();
            let path = dentry.path.unwrap();

            let send_fut = client.get(&raw_path).send();

            async move {
                match send_fut.await {
                    Ok(res) => match res.text().await {
                        Ok(text) => {
                            // println!("Received {} bytes from {}", text.len(), raw_path);
                            fs::write(path, text).expect("Unable to write file");
                        }
                        Err(_) => error!("when reading {}", raw_path),
                    },
                    Err(_) => error!("when downloading {}", raw_path),
                }
            }
        }))
        .buffer_unordered(4)
        .collect::<Vec<()>>();
        fetches.await;
    }

    Ok(())
}
