use std::fmt;
use std::io;

#[derive(Debug)]
pub struct Error {
    kind: ErrorKind,
}

#[derive(Debug)]
pub enum ErrorKind {
    DownloadFailure {
        path: String,
    },
    EmptyText,
    GitHubStatusFailure {
        status: reqwest::StatusCode,
        msg: String,
    },
    Interrupted,
    MalformedRepo {
        repo: String
    },
    ReadFailure {
        path: String,
    },
    ResponseKeyError {
        key: String
    },
    TreeDoesNotExist {
        tree: String,
        repo: String
    },
    HttpClientError(reqwest::Error),
    IoError(io::Error),
    Other {
        status: String,
    },
}

pub type Result<T> = ::std::result::Result<T, Box<Error>>;

impl Error {
    pub fn new(kind: ErrorKind) -> Box<Error> {
        Box::new(Error { kind })
    }

    pub fn err<T>(kind: ErrorKind) -> Result<T> {
        Err(Error::new(kind))
    }

    pub fn kind(&self) -> &ErrorKind {
        &self.kind
    }

    pub fn eprintln(&self) {
        use std::error::Error;
        fn eprint_cause(e: &dyn std::error::Error) {
            eprint!(": {}", e);
            if let Some(s) = e.source() {
                eprint_cause(s);
            }
        }

        eprint!("Error: {}", self);
        if let Some(s) = self.source() {
            eprint_cause(s);
        }
        eprintln!();
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.kind.fmt(f)
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        use ErrorKind::*;
        match self.kind() {
            HttpClientError(s) => Some(s),
            _ => None,
        }
    }
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use ErrorKind::*;
        match self {
            DownloadFailure { path } => write!(
                f,
                "Downloading from {} caused an error",
                path
            ),
            EmptyText => write!(f, "Text was not provided"),
            GitHubStatusFailure { status, msg } => write!(
                f,
                "GitHub API failure with response status {}: {}",
                status, msg
            ),
            Interrupted => write!(f, "Fzf was Interrupted"),
            MalformedRepo { repo } => write!(
                f,
                "The given repo {} is malformed.",
                repo 
            ),
            ReadFailure { path } => write!(
                f,
                "Reading from {} caused an error",
                path
            ),
            ResponseKeyError { key } => write!(
                f,
                "The response is missing the key: {}",
                key 
            ),
            TreeDoesNotExist { tree, repo } => write!(
                f,
                "The tree {} does not exist for repo {}. If you did not specify a tree, specify master (by default, the tree is main).",
                tree,
                repo
            ),
            HttpClientError(_) => write!(f, "Network request failure"),
            IoError(_) => write!(f, "I/O failure"),
            Other { status } => write!(f, "An error occured: {}", status),
        }
    }
}

impl From<reqwest::Error> for Box<Error> {
    fn from(err: reqwest::Error) -> Box<Error> {
        Error::new(ErrorKind::HttpClientError(err))
    }
}

impl From<io::Error> for Box<Error> {
    fn from(err: io::Error) -> Box<Error> {
        Error::new(ErrorKind::IoError(err))
    }
}
