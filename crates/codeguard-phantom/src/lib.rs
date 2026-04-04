pub mod cache;
pub mod checker;
pub mod known_packages;

use codeguard_ast::extract_file_info;
use codeguard_core::{Diagnostic, RuleCode};
use std::collections::HashSet;
use std::path::Path;
use tree_sitter::Tree;

pub use checker::PackageStatus;

pub struct PhantomLinter {
    checker: checker::PackageChecker,
}

impl PhantomLinter {
    pub fn new(config: &codeguard_core::Config) -> anyhow::Result<Self> {
        let checker = checker::PackageChecker::new(config)?;
        Ok(Self { checker })
    }

    /// Collect unique package names from a file
    pub fn collect_packages(&self, tree: &Tree, source: &str, path: &Path) -> Vec<String> {
        let info = extract_file_info(tree, source, path);
        let mut packages: Vec<String> = info
            .imports
            .iter()
            .map(|i| i.module.clone())
            .collect();
        packages.sort();
        packages.dedup();
        packages
    }

    /// Prefetch all packages (batch HTTP + cache)
    pub fn prefetch(&self, packages: &[String]) {
        self.checker.prefetch(packages);
    }

    /// Lint a single file (call after prefetch)
    pub fn lint(&self, tree: &Tree, source: &str, path: &Path) -> Vec<Diagnostic> {
        let info = extract_file_info(tree, source, path);
        let mut diagnostics = Vec::new();
        let mut seen = HashSet::new();

        for import in &info.imports {
            let pkg = &import.module;
            if !seen.insert(pkg.clone()) {
                continue;
            }

            // Skip stdlib modules
            if is_stdlib(pkg) {
                continue;
            }

            let status = self.checker.check(pkg);
            match status {
                PackageStatus::NotFound => {
                    let suggestion = self
                        .checker
                        .suggest_similar(pkg)
                        .map(|s| format!("did you mean '{s}'? Possible slopsquatting target"));
                    let mut d = Diagnostic::error(
                        RuleCode::new("PH001"),
                        import.span.clone(),
                        format!("import \"{pkg}\": package not found on PyPI"),
                    );
                    if let Some(s) = suggestion {
                        d = d.with_suggestion(s);
                    } else {
                        d = d.with_suggestion("possible slopsquatting target".to_string());
                    }
                    diagnostics.push(d);
                }
                PackageStatus::NotInstalled => {
                    diagnostics.push(
                        Diagnostic::warning(
                            RuleCode::new("PH002"),
                            import.span.clone(),
                            format!("import \"{pkg}\": package not installed in current env"),
                        )
                        .with_suggestion(format!("run: pip install {pkg}")),
                    );
                }
                PackageStatus::Suspicious(reasons) => {
                    let reason_str = reasons.join("; ");
                    let mut d = Diagnostic::warning(
                        RuleCode::new("PH003"),
                        import.span.clone(),
                        format!("import \"{pkg}\": suspicious package ({reason_str})"),
                    );
                    if let Some(s) = self.checker.suggest_similar(pkg) {
                        d = d.with_suggestion(format!("did you mean '{s}'?"));
                    }
                    diagnostics.push(d);
                }
                PackageStatus::Safe | PackageStatus::Stdlib => {}
                PackageStatus::Unknown(msg) => {
                    diagnostics.push(Diagnostic::info(
                        RuleCode::new("PH002"),
                        import.span.clone(),
                        format!("import \"{pkg}\": unable to verify ({msg})"),
                    ));
                }
            }
        }

        diagnostics
    }
}

fn is_stdlib(module: &str) -> bool {
    STDLIB_MODULES.contains(&module)
}

const STDLIB_MODULES: &[&str] = &[
    "abc", "aifc", "argparse", "array", "ast", "asynchat", "asyncio", "asyncore",
    "atexit", "audioop", "base64", "bdb", "binascii", "binhex", "bisect",
    "builtins", "bz2", "calendar", "cgi", "cgitb", "chunk", "cmath", "cmd",
    "code", "codecs", "codeop", "collections", "colorsys", "compileall",
    "concurrent", "configparser", "contextlib", "contextvars", "copy", "copyreg",
    "cProfile", "crypt", "csv", "ctypes", "curses", "dataclasses", "datetime",
    "dbm", "decimal", "difflib", "dis", "distutils", "doctest", "email",
    "encodings", "enum", "errno", "faulthandler", "fcntl", "filecmp", "fileinput",
    "fnmatch", "fractions", "ftplib", "functools", "gc", "getopt", "getpass",
    "gettext", "glob", "grp", "gzip", "hashlib", "heapq", "hmac", "html",
    "http", "idlelib", "imaplib", "imghdr", "imp", "importlib", "inspect", "io",
    "ipaddress", "itertools", "json", "keyword", "lib2to3", "linecache",
    "locale", "logging", "lzma", "mailbox", "mailcap", "marshal", "math",
    "mimetypes", "mmap", "modulefinder", "multiprocessing", "netrc", "nis",
    "nntplib", "numbers", "operator", "optparse", "os", "ossaudiodev",
    "pathlib", "pdb", "pickle", "pickletools", "pipes", "pkgutil", "platform",
    "plistlib", "poplib", "posix", "posixpath", "pprint", "profile", "pstats",
    "pty", "pwd", "py_compile", "pyclbr", "pydoc", "queue", "quopri", "random",
    "re", "readline", "reprlib", "resource", "rlcompleter", "runpy", "sched",
    "secrets", "select", "selectors", "shelve", "shlex", "shutil", "signal",
    "site", "smtpd", "smtplib", "sndhdr", "socket", "socketserver", "sqlite3",
    "ssl", "stat", "statistics", "string", "stringprep", "struct", "subprocess",
    "sunau", "symtable", "sys", "sysconfig", "syslog", "tabnanny", "tarfile",
    "telnetlib", "tempfile", "termios", "test", "textwrap", "threading", "time",
    "timeit", "tkinter", "token", "tokenize", "tomllib", "trace", "traceback",
    "tracemalloc", "tty", "turtle", "turtledemo", "types", "typing",
    "unicodedata", "unittest", "urllib", "uu", "uuid", "venv", "warnings",
    "wave", "weakref", "webbrowser", "winreg", "winsound", "wsgiref",
    "xdrlib", "xml", "xmlrpc", "zipapp", "zipfile", "zipimport", "zlib",
    "_thread", "__future__",
];
