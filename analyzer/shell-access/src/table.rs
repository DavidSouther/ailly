//! Filesystem-operand rules for supported utilities. The table is additive:
//! unknown utilities produce no accesses; under-reporting is preferred to
//! fabricated paths.

/// What a utility's positional operands are, once its flags are set aside.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Positionals {
    /// Every operand is a file the utility reads.
    Read,
    /// The first operand is a script or a pattern; the rest are files it reads.
    /// This is the rule that keeps `sed -n '1,220p' src/lib.rs` from reporting
    /// `1,220p` as a file.
    ScriptThenRead,
    /// Every operand but the last is read; the last is written.
    ReadThenWriteLast,
    /// Every operand is a destination.
    Write,
    /// Every operand is removed.
    Delete,
    /// Every operand is a directory the utility reads. Listing and traversal
    /// utilities take a directory as their subject, so a bare `src` here is the
    /// directory `src` rather than a guess that it is a file.
    ReadDirectory,
    /// Every operand is a directory the utility creates.
    WriteDirectory,
    /// Every operand is a directory the utility removes.
    DeleteDirectory,
    /// No operand names a file.
    None,
}

/// What a flag's value is.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FlagValue {
    /// The flag stands alone; any suffix is attached (`-i.bak`) rather than a
    /// following word.
    None,
    /// The next word (or `--flag=value`) is not a file.
    Ignored,
    /// The value names a file that is read.
    Read,
    /// The value names a file that is written.
    Write,
    /// The value names a directory that is written to.
    WriteDirectory,
    /// The next word is not a file, and remaining positionals follow this policy.
    ModeChanging(Positionals),
}

/// One declared flag: its spelling, what its value is, and any side effects on
/// how the remaining operands are read.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FlagSpec {
    pub(crate) name: &'static str,
    pub(crate) value: FlagValue,
    /// When set, this flag replaces the utility's positional policy. Used for
    /// flags that take no value (or whose value kind already accounts for the
    /// word) but still change what the operands mean — kept separate from
    /// [`FlagValue::ModeChanging`] when the value itself is a file.
    pub(crate) positionals: Option<Positionals>,
    /// When set, read operands become writes. A two-character name also matches
    /// its suffixed form, so `-i.bak` counts.
    pub(crate) in_place: bool,
}

const fn flag(name: &'static str, value: FlagValue) -> FlagSpec {
    FlagSpec {
        name,
        value,
        positionals: None,
        in_place: false,
    }
}

const fn flag_mode(name: &'static str, value: FlagValue, positionals: Positionals) -> FlagSpec {
    FlagSpec {
        name,
        value,
        positionals: Some(positionals),
        in_place: false,
    }
}

pub(crate) struct Utility {
    pub(crate) positionals: Positionals,
    pub(crate) flags: &'static [FlagSpec],
    /// True when this utility runs a script handed to it inline. Its script's
    /// internals are not attributed; redirects around it still are.
    pub(crate) scripting: bool,
    /// True when only the leading operands are paths: the first word this table
    /// does not declare opens an expression, as `-name` does in
    /// `find src -name '*.rs'`, and nothing past it is a name.
    pub(crate) expression: bool,
}

/// A command that carries another command.
pub(crate) enum Wrapper {
    /// Assignments, flags, and a fixed number of leading operands belong to the
    /// wrapper; what follows is the command it runs.
    Prefix {
        value_flags: &'static [&'static str],
        skip_operands: usize,
    },
    /// A `-c` flag hands a shell one string to read as a command of its own.
    CommandString,
}

const fn utility(positionals: Positionals, flags: &'static [FlagSpec]) -> Utility {
    Utility {
        positionals,
        flags,
        scripting: false,
        expression: false,
    }
}

const READER: Utility = utility(Positionals::Read, &[]);
const HEAD: Utility = utility(
    Positionals::Read,
    &[
        flag("-n", FlagValue::Ignored),
        flag("-c", FlagValue::Ignored),
        flag("--lines", FlagValue::Ignored),
        flag("--bytes", FlagValue::Ignored),
        flag("--quiet", FlagValue::None),
    ],
);
const OD: Utility = utility(
    Positionals::Read,
    &[
        flag("-A", FlagValue::Ignored),
        flag("-j", FlagValue::Ignored),
        flag("-N", FlagValue::Ignored),
        flag("-t", FlagValue::Ignored),
        flag("-w", FlagValue::Ignored),
        flag("-S", FlagValue::Ignored),
    ],
);
const SORT: Utility = utility(
    Positionals::Read,
    &[
        flag("-k", FlagValue::Ignored),
        flag("-t", FlagValue::Ignored),
        flag("-o", FlagValue::Write),
        flag("-S", FlagValue::Ignored),
        flag("-T", FlagValue::Ignored),
        flag("--key", FlagValue::Ignored),
        flag("--field-separator", FlagValue::Ignored),
        flag("--output", FlagValue::Write),
        flag("--buffer-size", FlagValue::Ignored),
        flag("--temporary-directory", FlagValue::Ignored),
    ],
);
const CUT: Utility = utility(
    Positionals::Read,
    &[
        flag("-d", FlagValue::Ignored),
        flag("-f", FlagValue::Ignored),
        flag("-b", FlagValue::Ignored),
        flag("-c", FlagValue::Ignored),
        flag("--delimiter", FlagValue::Ignored),
        flag("--fields", FlagValue::Ignored),
        flag("--bytes", FlagValue::Ignored),
        flag("--characters", FlagValue::Ignored),
        flag("--output-delimiter", FlagValue::Ignored),
    ],
);

const SED: Utility = Utility {
    positionals: Positionals::ScriptThenRead,
    flags: &[
        flag("-e", FlagValue::ModeChanging(Positionals::Read)),
        flag("--expression", FlagValue::ModeChanging(Positionals::Read)),
        flag_mode("-f", FlagValue::Read, Positionals::Read),
        flag_mode("--file", FlagValue::Read, Positionals::Read),
        flag("-l", FlagValue::Ignored),
        flag("--line-length", FlagValue::Ignored),
        FlagSpec {
            name: "-i",
            value: FlagValue::None,
            positionals: None,
            in_place: true,
        },
        FlagSpec {
            name: "--in-place",
            value: FlagValue::None,
            positionals: None,
            in_place: true,
        },
    ],
    scripting: false,
    expression: false,
};

const AWK: Utility = Utility {
    positionals: Positionals::ScriptThenRead,
    flags: &[
        flag_mode("-f", FlagValue::Read, Positionals::Read),
        flag("-v", FlagValue::Ignored),
        flag_mode("--file", FlagValue::Read, Positionals::Read),
        flag("--assign", FlagValue::Ignored),
        flag("-F", FlagValue::Ignored),
        flag("--field-separator", FlagValue::Ignored),
    ],
    scripting: false,
    expression: false,
};

const GREP: Utility = Utility {
    positionals: Positionals::ScriptThenRead,
    flags: &[
        flag("-e", FlagValue::ModeChanging(Positionals::Read)),
        flag_mode("-f", FlagValue::Read, Positionals::Read),
        flag("-m", FlagValue::Ignored),
        flag("-A", FlagValue::Ignored),
        flag("-B", FlagValue::Ignored),
        flag("-C", FlagValue::Ignored),
        flag("-d", FlagValue::Ignored),
        flag("-D", FlagValue::Ignored),
        flag("--regexp", FlagValue::ModeChanging(Positionals::Read)),
        flag_mode("--file", FlagValue::Read, Positionals::Read),
        flag("--max-count", FlagValue::Ignored),
        flag("--after-context", FlagValue::Ignored),
        flag("--before-context", FlagValue::Ignored),
        flag("--context", FlagValue::Ignored),
        flag("--include", FlagValue::Ignored),
        flag("--exclude", FlagValue::Ignored),
        flag("--exclude-dir", FlagValue::Ignored),
        flag("--label", FlagValue::Ignored),
        flag("--binary-files", FlagValue::Ignored),
        flag("--devices", FlagValue::Ignored),
        flag("--directories", FlagValue::Ignored),
        flag("--color", FlagValue::Ignored),
        flag("--colour", FlagValue::Ignored),
    ],
    scripting: false,
    expression: false,
};

const RIPGREP: Utility = Utility {
    positionals: Positionals::ScriptThenRead,
    flags: &[
        flag("-e", FlagValue::ModeChanging(Positionals::Read)),
        flag_mode("-f", FlagValue::Read, Positionals::Read),
        flag("-m", FlagValue::Ignored),
        flag("-A", FlagValue::Ignored),
        flag("-B", FlagValue::Ignored),
        flag("-C", FlagValue::Ignored),
        flag("-g", FlagValue::Ignored),
        flag("-t", FlagValue::Ignored),
        flag("-T", FlagValue::Ignored),
        flag("-M", FlagValue::Ignored),
        flag("-r", FlagValue::Ignored),
        flag("--regexp", FlagValue::ModeChanging(Positionals::Read)),
        flag_mode("--file", FlagValue::Read, Positionals::Read),
        flag("--max-count", FlagValue::Ignored),
        flag("--after-context", FlagValue::Ignored),
        flag("--before-context", FlagValue::Ignored),
        flag("--context", FlagValue::Ignored),
        flag("--glob", FlagValue::Ignored),
        flag("--iglob", FlagValue::Ignored),
        flag("--type", FlagValue::Ignored),
        flag("--type-not", FlagValue::Ignored),
        flag("--max-columns", FlagValue::Ignored),
        flag("--max-depth", FlagValue::Ignored),
        flag("--replace", FlagValue::Ignored),
        flag("--sort", FlagValue::Ignored),
        flag("--sortr", FlagValue::Ignored),
        flag("--color", FlagValue::Ignored),
        flag("--colors", FlagValue::Ignored),
        flag("--pre", FlagValue::Ignored),
    ],
    scripting: false,
    expression: false,
};

/// `ack` and `ag` take the same context and count flags as `rg`, but not its
/// `-r`: there `-r` is recursion and takes no value, so borrowing ripgrep's
/// declaration would consume the search pattern and leave the command silent.
const ACK: Utility = Utility {
    positionals: Positionals::ScriptThenRead,
    flags: &[
        flag("-A", FlagValue::Ignored),
        flag("-B", FlagValue::Ignored),
        flag("-C", FlagValue::Ignored),
        flag("-m", FlagValue::Ignored),
        flag("-G", FlagValue::Ignored),
        flag("--max-count", FlagValue::Ignored),
        flag("--after-context", FlagValue::Ignored),
        flag("--before-context", FlagValue::Ignored),
        flag("--context", FlagValue::Ignored),
        flag("--ignore-dir", FlagValue::Ignored),
        flag("--ignore-file", FlagValue::Ignored),
        flag("--color", FlagValue::Ignored),
        flag("--colour", FlagValue::Ignored),
    ],
    scripting: false,
    expression: false,
};

const JQ: Utility = Utility {
    positionals: Positionals::ScriptThenRead,
    flags: &[
        flag_mode("-f", FlagValue::Read, Positionals::Read),
        flag_mode("--from-file", FlagValue::Read, Positionals::Read),
        flag("--indent", FlagValue::Ignored),
        flag("--arg", FlagValue::Ignored),
        flag("--argjson", FlagValue::Ignored),
    ],
    scripting: false,
    expression: false,
};

const TEE: Utility = utility(Positionals::Write, &[]);
const TOUCH: Utility = utility(
    Positionals::Write,
    &[
        flag("-d", FlagValue::Ignored),
        flag("-t", FlagValue::Ignored),
        flag("-r", FlagValue::Read),
        flag("--date", FlagValue::Ignored),
        flag("--time", FlagValue::Ignored),
        flag("--reference", FlagValue::Read),
    ],
);
const TRUNCATE: Utility = utility(
    Positionals::Write,
    &[
        flag("-s", FlagValue::Ignored),
        flag("-r", FlagValue::Read),
        flag("--size", FlagValue::Ignored),
        flag("--reference", FlagValue::Read),
        flag("-o", FlagValue::None),
        flag("--io-blocks", FlagValue::None),
    ],
);

/// `cp`/`mv`: read all operands except the destination; `-t` supplies the
/// destination directory.
const COPY: Utility = Utility {
    positionals: Positionals::ReadThenWriteLast,
    flags: &[
        flag_mode("-t", FlagValue::WriteDirectory, Positionals::Read),
        flag("-S", FlagValue::Ignored),
        flag_mode(
            "--target-directory",
            FlagValue::WriteDirectory,
            Positionals::Read,
        ),
        flag("--suffix", FlagValue::Ignored),
    ],
    scripting: false,
    expression: false,
};

/// `install` places files like `cp`, but its mode and ownership flags take
/// values that are not paths, and `-d` makes every operand a directory it
/// creates instead of a file it writes. Declaring them is what keeps `root` in
/// `install -o root src dst` from being reported as a file.
const INSTALL: Utility = Utility {
    positionals: Positionals::ReadThenWriteLast,
    flags: &[
        flag_mode("-t", FlagValue::WriteDirectory, Positionals::Read),
        flag_mode(
            "--target-directory",
            FlagValue::WriteDirectory,
            Positionals::Read,
        ),
        flag_mode("-d", FlagValue::None, Positionals::WriteDirectory),
        flag_mode("--directory", FlagValue::None, Positionals::WriteDirectory),
        flag("-m", FlagValue::Ignored),
        flag("-o", FlagValue::Ignored),
        flag("-g", FlagValue::Ignored),
        flag("-S", FlagValue::Ignored),
        flag("--mode", FlagValue::Ignored),
        flag("--owner", FlagValue::Ignored),
        flag("--group", FlagValue::Ignored),
        flag("--suffix", FlagValue::Ignored),
    ],
    scripting: false,
    expression: false,
};

const REMOVE: Utility = utility(Positionals::Delete, &[]);
const MAKE_DIRECTORY: Utility = utility(Positionals::WriteDirectory, &[]);
const REMOVE_DIRECTORY: Utility = utility(Positionals::DeleteDirectory, &[]);

/// Classify `ls` operands as directories for this model, although `ls` also
/// accepts files. This is a command-semantics classification, not filesystem
/// inspection.
const LIST: Utility = utility(
    Positionals::ReadDirectory,
    &[
        flag("-I", FlagValue::Ignored),
        flag("-w", FlagValue::Ignored),
        flag("--ignore", FlagValue::Ignored),
        flag("--width", FlagValue::Ignored),
        flag("--format", FlagValue::Ignored),
        flag("--sort", FlagValue::Ignored),
        flag("--time-style", FlagValue::Ignored),
        flag("--block-size", FlagValue::Ignored),
        flag("--color", FlagValue::Ignored),
    ],
);

/// `find`: classify leading roots only; stop at the expression.
const FIND: Utility = Utility {
    positionals: Positionals::ReadDirectory,
    flags: &[
        flag("-L", FlagValue::None),
        flag("-H", FlagValue::None),
        flag("-P", FlagValue::None),
        flag("-E", FlagValue::None),
        flag("-d", FlagValue::None),
        flag("-s", FlagValue::None),
        flag("-x", FlagValue::None),
    ],
    scripting: false,
    expression: true,
};

const DISK_USAGE: Utility = utility(
    Positionals::ReadDirectory,
    &[
        flag("-d", FlagValue::Ignored),
        flag("-t", FlagValue::Ignored),
        flag("-B", FlagValue::Ignored),
        flag("-I", FlagValue::Ignored),
        flag("--max-depth", FlagValue::Ignored),
        flag("--threshold", FlagValue::Ignored),
        flag("--block-size", FlagValue::Ignored),
        flag("--exclude", FlagValue::Ignored),
    ],
);

const TREE: Utility = utility(
    Positionals::ReadDirectory,
    &[
        flag("-L", FlagValue::Ignored),
        flag("-I", FlagValue::Ignored),
        flag("-P", FlagValue::Ignored),
        flag("-o", FlagValue::Write),
        flag("--filelimit", FlagValue::Ignored),
    ],
);

/// Inline interpreter arguments are not attributed; surrounding redirects are.
const INTERPRETER: Utility = Utility {
    positionals: Positionals::None,
    flags: &[
        flag("-c", FlagValue::Ignored),
        flag("-e", FlagValue::Ignored),
        flag("-E", FlagValue::Ignored),
        flag("-m", FlagValue::Ignored),
        flag("--eval", FlagValue::Ignored),
        flag("--exec", FlagValue::Ignored),
        flag("--command", FlagValue::Ignored),
    ],
    scripting: true,
    expression: false,
};

const PREFIX: Wrapper = Wrapper::Prefix {
    value_flags: &[],
    skip_operands: 0,
};

/// The utility this name is known to be, or `None` for the visible
/// under-report.
pub(crate) fn lookup(name: &str) -> Option<&'static Utility> {
    Some(match name {
        "cat" | "tac" | "less" | "more" | "wc" | "nl" | "strings" | "file" | "xxd" | "hexdump"
        | "base64" | "md5sum" | "sha1sum" | "sha256sum" | "sha512sum" | "cksum" | "uniq"
        | "expand" | "unexpand" | "fold" | "paste" | "comm" | "diff" | "diff3" | "cmp"
        | "iconv" | "shasum" => &READER,
        "head" | "tail" => &HEAD,
        "od" => &OD,
        "sort" => &SORT,
        "cut" => &CUT,
        "sed" => &SED,
        "awk" | "gawk" | "nawk" | "mawk" => &AWK,
        "grep" | "egrep" | "fgrep" => &GREP,
        "rg" => &RIPGREP,
        "ag" | "ack" => &ACK,
        "jq" | "yq" => &JQ,
        "tee" => &TEE,
        "touch" => &TOUCH,
        "truncate" => &TRUNCATE,
        "cp" | "mv" | "ln" => &COPY,
        "install" => &INSTALL,
        "rm" | "unlink" | "shred" => &REMOVE,
        "ls" | "dir" | "vdir" => &LIST,
        "find" => &FIND,
        "du" => &DISK_USAGE,
        "tree" => &TREE,
        "mkdir" => &MAKE_DIRECTORY,
        "rmdir" => &REMOVE_DIRECTORY,
        "python" | "python3" | "perl" | "ruby" | "node" | "deno" | "bun" | "php" | "lua"
        | "Rscript" | "osascript" => &INTERPRETER,
        _ => return None,
    })
}

/// The wrapper this name is known to be, for the wrapper shapes the corpus
/// actually contains.
pub(crate) fn wrapper(name: &str) -> Option<&'static Wrapper> {
    Some(match name {
        "env" => &Wrapper::Prefix {
            value_flags: &["-u", "-S", "--unset", "--chdir", "-C"],
            skip_operands: 0,
        },
        "sudo" => &Wrapper::Prefix {
            value_flags: &[
                "-u", "-g", "-p", "-C", "-r", "-t", "-U", "-h", "--user", "--group", "--prompt",
                "--role", "--type", "--host",
            ],
            skip_operands: 0,
        },
        "timeout" => &Wrapper::Prefix {
            value_flags: &["-s", "-k", "--signal", "--kill-after"],
            skip_operands: 1,
        },
        "nice" => &Wrapper::Prefix {
            value_flags: &["-n", "--adjustment"],
            skip_operands: 0,
        },
        "xargs" => &Wrapper::Prefix {
            value_flags: &[
                "-n",
                "-P",
                "-I",
                "-i",
                "-d",
                "-L",
                "-s",
                "-E",
                "--max-args",
                "--max-procs",
                "--replace",
                "--delimiter",
                "--max-lines",
                "--max-chars",
            ],
            skip_operands: 0,
        },
        "nohup" | "command" | "stdbuf" | "time" => &PREFIX,
        "sh" | "bash" | "zsh" | "dash" | "ksh" => &Wrapper::CommandString,
        _ => return None,
    })
}

/// The declared flag matching this spelling, including a short flag's suffixed
/// form when the declaration is in-place (`-i.bak` for `-i`).
pub(crate) fn find_flag<'a>(flags: &'a [FlagSpec], text: &str) -> Option<&'a FlagSpec> {
    if let Some(spec) = flags.iter().find(|spec| spec.name == text) {
        return Some(spec);
    }
    flags.iter().find(|spec| {
        spec.in_place && spec.name.len() == 2 && text.starts_with(spec.name) && text != spec.name
    })
}
