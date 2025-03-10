// Copyright (c) The nextest Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Errors produced by nextest.

use crate::{
    cargo_config::TargetTriple,
    helpers::{dylib_path_envvar, extract_abort_status},
    reporter::{StatusLevel, TestOutputDisplay},
    reuse_build::ArchiveFormat,
    runner::AbortStatus,
    target_runner::PlatformRunnerSource,
    test_filter::RunIgnored,
};
use camino::{FromPathBufError, Utf8Path, Utf8PathBuf};
use config::ConfigError;
use itertools::Itertools;
use nextest_filtering::errors::FilterExpressionParseErrors;
use std::{borrow::Cow, env::JoinPathsError, fmt, process::ExitStatus};
use thiserror::Error;

/// An error that occurred while parsing the config.
#[derive(Debug, Error)]
#[error(
    "failed to parse nextest config at `{config_file}`{}",
    provided_by_tool(tool.as_deref())
)]
#[non_exhaustive]
pub struct ConfigParseError {
    config_file: Utf8PathBuf,
    tool: Option<String>,
    #[source]
    kind: ConfigParseErrorKind,
}

impl ConfigParseError {
    pub(crate) fn new(
        config_file: impl Into<Utf8PathBuf>,
        tool: Option<&str>,
        kind: ConfigParseErrorKind,
    ) -> Self {
        Self {
            config_file: config_file.into(),
            tool: tool.map(|s| s.to_owned()),
            kind,
        }
    }

    /// Returns the config file for this error.
    pub fn config_file(&self) -> &Utf8Path {
        &self.config_file
    }

    /// Returns the kind of error this is.
    pub fn kind(&self) -> &ConfigParseErrorKind {
        &self.kind
    }
}

fn provided_by_tool(tool: Option<&str>) -> String {
    match tool {
        Some(tool) => format!(" provided by tool `{tool}`"),
        None => String::new(),
    }
}

/// The kind of error that occurred while parsing a config.
///
/// Returned by [`ConfigParseError::kind`].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ConfigParseErrorKind {
    /// An error occurred while building the config.
    #[error(transparent)]
    BuildError(ConfigError),
    #[error(transparent)]
    /// An error occurred while deserializing the config.
    DeserializeError(serde_path_to_error::Error<ConfigError>),
    /// Errors occurred while parsing overrides.
    #[error("error parsing overrides (destructure this variant for more details)")]
    OverrideError(Vec<ConfigParseOverrideError>),
}

/// An error that occurred while parsing config overrides.
///
/// Part of [`ConfigParseErrorKind::OverrideError`].
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct ConfigParseOverrideError {
    /// The name of the profile under which the override was found.
    pub profile_name: String,

    /// The expression, and the errors that occurred.
    pub parse_errors: FilterExpressionParseErrors,
}

/// An error which indicates that a profile was requested but not known to nextest.
#[derive(Clone, Debug, Error)]
#[error("profile `{profile} not found (known profiles: {})`", .all_profiles.join(", "))]
pub struct ProfileNotFound {
    profile: String,
    all_profiles: Vec<String>,
}

impl ProfileNotFound {
    pub(crate) fn new(
        profile: impl Into<String>,
        all_profiles: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        let mut all_profiles: Vec<_> = all_profiles.into_iter().map(|s| s.into()).collect();
        all_profiles.sort_unstable();
        Self {
            profile: profile.into(),
            all_profiles,
        }
    }
}

/// Error returned while parsing a [`TestOutputDisplay`] value from a string.
#[derive(Clone, Debug, Error)]
#[error(
    "unrecognized value for test output display: {input}\n(known values: {})",
    TestOutputDisplay::variants().join(", "),
)]
pub struct TestOutputDisplayParseError {
    input: String,
}

impl TestOutputDisplayParseError {
    pub(crate) fn new(input: impl Into<String>) -> Self {
        Self {
            input: input.into(),
        }
    }
}

/// Error returned while parsing a [`StatusLevel`] value from a string.
#[derive(Clone, Debug, Error)]
#[error(
    "unrecognized value for status-level: {input}\n(known values: {})",
    StatusLevel::variants().join(", "),
)]
pub struct StatusLevelParseError {
    input: String,
}

impl StatusLevelParseError {
    pub(crate) fn new(input: impl Into<String>) -> Self {
        Self {
            input: input.into(),
        }
    }
}

/// Error returned while parsing a [`ToolConfigFile`](crate::config::ToolConfigFile) value.
#[derive(Clone, Debug, Error)]
pub enum ToolConfigFileParseError {
    #[error(
        "tool-config-file has invalid format: {input}\n(hint: tool configs must be in the format <tool-name>:<path>)"
    )]
    /// The input was not in the format "tool:path".
    InvalidFormat {
        /// The input that failed to parse.
        input: String,
    },

    /// The tool name was empty.
    #[error("tool-config-file has empty tool name: {input}")]
    EmptyToolName {
        /// The input that failed to parse.
        input: String,
    },

    /// The config file path was empty.
    #[error("tool-config-file has empty config file path: {input}")]
    EmptyConfigFile {
        /// The input that failed to parse.
        input: String,
    },

    /// The config file was not an absolute path.
    #[error("tool-config-file is not an absolute path: {config_file}")]
    ConfigFileNotAbsolute {
        /// The file name that wasn't absolute.
        config_file: Utf8PathBuf,
    },
}

/// Error returned while parsing a [`TestThreads`](crate::config::TestThreads) value.
#[derive(Clone, Debug, Error)]
#[error(
    "unrecognized value for test-threads: {input}\n(hint: expected either an integer or \"num-cpus\")"
)]
pub struct TestThreadsParseError {
    /// The input that failed to parse.
    pub input: String,
}

impl TestThreadsParseError {
    pub(crate) fn new(input: impl Into<String>) -> Self {
        Self {
            input: input.into(),
        }
    }
}

/// An error that occurs while parsing a [`RunIgnored`] value from a string.
#[derive(Clone, Debug, Error)]
#[error(
    "unrecognized value for run-ignored: {input}\n(known values: {})",
    RunIgnored::variants().join(", "),
)]

pub struct RunIgnoredParseError {
    input: String,
}

impl RunIgnoredParseError {
    pub(crate) fn new(input: impl Into<String>) -> Self {
        Self {
            input: input.into(),
        }
    }
}

/// An error that occurs while parsing a
/// [`PartitionerBuilder`](crate::partition::PartitionerBuilder) input.
#[derive(Clone, Debug, Error)]
pub struct PartitionerBuilderParseError {
    expected_format: Option<&'static str>,
    message: Cow<'static, str>,
}

impl PartitionerBuilderParseError {
    pub(crate) fn new(
        expected_format: Option<&'static str>,
        message: impl Into<Cow<'static, str>>,
    ) -> Self {
        Self {
            expected_format,
            message: message.into(),
        }
    }
}

impl fmt::Display for PartitionerBuilderParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.expected_format {
            Some(format) => {
                write!(
                    f,
                    "partition must be in the format \"{}\":\n{}",
                    format, self.message
                )
            }
            None => write!(f, "{}", self.message),
        }
    }
}

/// An error occurred in [`PathMapper::new`](crate::reuse_build::PathMapper::new).
#[derive(Debug, Error)]
pub enum PathMapperConstructError {
    /// An error occurred while canonicalizing a directory.
    #[error("{kind} `{input}` failed to canonicalize")]
    Canonicalization {
        /// The directory that failed to be canonicalized.
        kind: PathMapperConstructKind,

        /// The input provided.
        input: Utf8PathBuf,

        /// The error that occurred.
        #[source]
        err: std::io::Error,
    },
    /// The canonicalized path isn't valid UTF-8.
    #[error("{kind} `{input}` canonicalized to a non-UTF-8 path")]
    NonUtf8Path {
        /// The directory that failed to be canonicalized.
        kind: PathMapperConstructKind,

        /// The input provided.
        input: Utf8PathBuf,

        /// The underlying error.
        #[source]
        err: FromPathBufError,
    },
    /// A provided input is not a directory.
    #[error("{kind} `{canonicalized_path}` is not a directory")]
    NotADirectory {
        /// The directory that failed to be canonicalized.
        kind: PathMapperConstructKind,

        /// The input provided.
        input: Utf8PathBuf,

        /// The canonicalized path that wasn't a directory.
        canonicalized_path: Utf8PathBuf,
    },
}

impl PathMapperConstructError {
    /// The kind of directory.
    pub fn kind(&self) -> PathMapperConstructKind {
        match self {
            Self::Canonicalization { kind, .. }
            | Self::NonUtf8Path { kind, .. }
            | Self::NotADirectory { kind, .. } => *kind,
        }
    }

    /// The input path that failed.
    pub fn input(&self) -> &Utf8Path {
        match self {
            Self::Canonicalization { input, .. }
            | Self::NonUtf8Path { input, .. }
            | Self::NotADirectory { input, .. } => input,
        }
    }
}

/// The kind of directory that failed to be read in
/// [`PathMapper::new`](crate::reuse_build::PathMapper::new).
///
/// Returned as part of [`PathMapperConstructError`].
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PathMapperConstructKind {
    /// The workspace root.
    WorkspaceRoot,

    /// The target directory.
    TargetDir,
}

impl fmt::Display for PathMapperConstructKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::WorkspaceRoot => write!(f, "remapped workspace root"),
            Self::TargetDir => write!(f, "remapped target directory"),
        }
    }
}

/// An error that occurs in [`BinaryList::from_messages`](crate::list::BinaryList::from_messages) or
/// [`RustTestArtifact::from_binary_list`](crate::list::RustTestArtifact::from_binary_list).
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum FromMessagesError {
    /// An error occurred while reading Cargo's JSON messages.
    #[error("error reading Cargo JSON messages")]
    ReadMessages(#[source] std::io::Error),

    /// An error occurred while querying the package graph.
    #[error("error querying package graph")]
    PackageGraph(#[source] guppy::Error),

    /// A target in the package graph was missing `kind` information.
    #[error("missing kind for target {binary_name} in package {package_name}")]
    MissingTargetKind {
        /// The name of the malformed package.
        package_name: String,
        /// The name of the malformed target within the package.
        binary_name: String,
    },
}

/// An error that occurs while parsing test list output.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum CreateTestListError {
    /// The proposed cwd for a process is not a directory.
    #[error(
        "for `{binary_id}`, current directory `{cwd}` is not a directory\n\
         (hint: ensure project source is available at this location)"
    )]
    CwdIsNotDir {
        /// The binary ID for which the current directory wasn't found.
        binary_id: String,

        /// The current directory that wasn't found.
        cwd: Utf8PathBuf,
    },

    /// Running a command to gather the list of tests failed to execute.
    #[error(
        "for `{binary_id}`, running command `{}` failed to execute",
        shell_words::join(command)
    )]
    CommandExecFail {
        /// The binary ID for which gathering the list of tests failed.
        binary_id: String,

        /// The command that was run.
        command: Vec<String>,

        /// The underlying error.
        #[source]
        error: std::io::Error,
    },

    /// Running a command to gather the list of tests failed failed with a non-zero exit code.
    #[error(
        "for `{binary_id}`, command `{}` exited with {}\n--- stdout:\n{}\n--- stderr:\n{}\n---",
        shell_words::join(command),
        display_exit_status(*exit_status),
        String::from_utf8_lossy(stdout),
        String::from_utf8_lossy(stderr),
    )]
    CommandFail {
        /// The binary ID for which gathering the list of tests failed.
        binary_id: String,

        /// The command that was run.
        command: Vec<String>,

        /// The exit status with which the command failed.
        exit_status: ExitStatus,

        /// Standard output for the command.
        stdout: Vec<u8>,

        /// Standard error for the command.
        stderr: Vec<u8>,
    },

    /// Running a command to gather the list of tests produced a non-UTF-8 standard output.
    #[error(
        "for `{binary_id}`, command `{}` produced non-UTF-8 output: {}\n--- stdout:\n{}\n--- stderr:\n{}\n---",
        shell_words::join(command),
        String::from_utf8_lossy(stdout),
        String::from_utf8_lossy(stderr),
    )]
    CommandNonUtf8 {
        /// The binary ID for which gathering the list of tests failed.
        binary_id: String,

        /// The command that was run.
        command: Vec<String>,

        /// Standard output for the command.
        stdout: Vec<u8>,

        /// Standard error for the command.
        stderr: Vec<u8>,
    },

    /// An error occurred while parsing a line in the test output.
    #[error("for `{binary_id}`, {message}\nfull output:\n{full_output}")]
    ParseLine {
        /// The binary ID for which parsing the list of tests failed.
        binary_id: String,

        /// A descriptive message.
        message: Cow<'static, str>,

        /// The full output.
        full_output: String,
    },

    /// An error occurred while joining paths for dynamic libraries.
    #[error(
        "error joining dynamic library paths for {}: [{}]",
        dylib_path_envvar(),
        itertools::join(.new_paths, ", ")
    )]
    DylibJoinPaths {
        /// New paths attempted to be added to the dynamic library environment variable.
        new_paths: Vec<Utf8PathBuf>,

        /// The underlying error.
        #[source]
        error: JoinPathsError,
    },

    /// Creating a Tokio runtime failed.
    #[error("error creating Tokio runtime")]
    TokioRuntimeCreate(#[source] std::io::Error),
}

impl CreateTestListError {
    pub(crate) fn parse_line(
        binary_id: impl Into<String>,
        message: impl Into<Cow<'static, str>>,
        full_output: impl Into<String>,
    ) -> Self {
        Self::ParseLine {
            binary_id: binary_id.into(),
            message: message.into(),
            full_output: full_output.into(),
        }
    }

    pub(crate) fn dylib_join_paths(new_paths: Vec<Utf8PathBuf>, error: JoinPathsError) -> Self {
        Self::DylibJoinPaths { new_paths, error }
    }
}

fn display_exit_status(exit_status: ExitStatus) -> String {
    match extract_abort_status(exit_status) {
        #[cfg(unix)]
        Some(AbortStatus::UnixSignal(sig)) => match crate::helpers::signal_str(sig) {
            Some(s) => {
                format!("signal {sig} (SIG{s})")
            }
            None => {
                format!("signal {sig}")
            }
        },
        #[cfg(windows)]
        Some(AbortStatus::WindowsNtStatus(nt_status)) => {
            format!("code {}", crate::helpers::display_nt_status(nt_status))
        }
        None => match exit_status.code() {
            Some(code) => {
                format!("code {code}")
            }
            None => "an unknown error".to_owned(),
        },
    }
}

/// An error that occurs while writing list output.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum WriteTestListError {
    /// An error occurred while writing the list to the provided output.
    #[error("error writing to output")]
    Io(#[source] std::io::Error),

    /// An error occurred while serializing JSON, or while writing it to the provided output.
    #[error("error serializing to JSON")]
    Json(#[source] serde_json::Error),
}

/// An error occurred while configuring handles.
///
/// Only relevant on Windows.
#[derive(Debug, Error)]
pub enum ConfigureHandleInheritanceError {
    /// An error occurred. This can only happen on Windows.
    #[cfg(windows)]
    #[error("error configuring handle inheritance")]
    WindowsError(#[from] windows::core::Error),
}

/// An error that occurs while building the test runner.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum TestRunnerBuildError {
    /// An error occurred while creating the Tokio runtime.
    #[error("error creating Tokio runtime")]
    TokioRuntimeCreate(#[source] std::io::Error),

    /// An error occurred while setting up signals.
    #[error("error setting up signals")]
    SignalHandlerSetupError(#[from] SignalHandlerSetupError),
}

/// Represents an unknown archive format.
///
/// Returned by [`ArchiveFormat::autodetect`].
#[derive(Debug, Error)]
#[error(
    "could not detect archive format from file name `{file_name}` (supported extensions: {})",
    supported_extensions()
)]
pub struct UnknownArchiveFormat {
    /// The name of the archive file without any leading components.
    pub file_name: String,
}

fn supported_extensions() -> String {
    ArchiveFormat::SUPPORTED_FORMATS
        .iter()
        .map(|(extension, _)| *extension)
        .join(", ")
}

/// An error that occurs while archiving data.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ArchiveCreateError {
    /// An error occurred while creating the binary list to be written.
    #[error("error creating binary list")]
    CreateBinaryList(#[source] WriteTestListError),

    /// An error occurred while reading data from a file on disk.
    #[error("error writing {} `{path}` to archive", kind_str(*.is_dir))]
    InputFileRead {
        /// The name of the file that could not be read.
        path: Utf8PathBuf,

        /// Whether this is a directory. `None` means the status was unknown.
        is_dir: Option<bool>,

        /// The error that occurred.
        #[source]
        error: std::io::Error,
    },

    /// An error occurred while reading entries from a directory on disk.
    #[error("error reading directory entry from `{path}")]
    DirEntryRead {
        /// The name of the directory from which entries couldn't be read.
        path: Utf8PathBuf,

        /// The error that occurred.
        #[source]
        error: std::io::Error,
    },

    /// An error occurred while writing data to the output file.
    #[error("error writing to archive")]
    OutputArchiveIo(#[source] std::io::Error),

    /// An error occurred in the reporter.
    #[error("error reporting archive status")]
    ReporterIo(#[source] std::io::Error),
}

fn kind_str(is_dir: Option<bool>) -> &'static str {
    match is_dir {
        Some(true) => "directory",
        Some(false) => "file",
        None => "path",
    }
}

/// An error occurred while reading a file.
///
/// Returned as part of both [`ArchiveCreateError`] and [`ArchiveExtractError`].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ArchiveReadError {
    /// An I/O error occurred while reading the archive.
    #[error("I/O error reading archive")]
    Io(#[source] std::io::Error),

    /// A path wasn't valid UTF-8.
    #[error("path in archive `{}` wasn't valid UTF-8", String::from_utf8_lossy(.0))]
    NonUtf8Path(Vec<u8>),

    /// A file path within the archive didn't begin with "target/".
    #[error("path in archive `{0}` doesn't start with `target/`")]
    NoTargetPrefix(Utf8PathBuf),

    /// A file path within the archive had an invalid component within it.
    #[error("path in archive `{path}` contains an invalid component `{component}`")]
    InvalidComponent {
        /// The path that had an invalid component.
        path: Utf8PathBuf,

        /// The invalid component.
        component: String,
    },

    /// An error occurred while reading a checksum.
    #[error("corrupted archive: checksum read error for path `{path}`")]
    ChecksumRead {
        /// The path for which there was a checksum read error.
        path: Utf8PathBuf,

        /// The error that occurred.
        #[source]
        error: std::io::Error,
    },

    /// An entry had an invalid checksum.
    #[error("corrupted archive: invalid checksum for path `{path}`")]
    InvalidChecksum {
        /// The path that had an invalid checksum.
        path: Utf8PathBuf,

        /// The expected checksum.
        expected: u32,

        /// The actual checksum.
        actual: u32,
    },

    /// A metadata file wasn't found.
    #[error("metadata file `{0}` not found in archive")]
    MetadataFileNotFound(&'static Utf8Path),

    /// An error occurred while deserializing a metadata file.
    #[error("error deserializing metadata file `{path}` in archive")]
    MetadataDeserializeError {
        /// The name of the metadata file.
        path: &'static Utf8Path,

        /// The deserialize error.
        #[source]
        error: serde_json::Error,
    },

    /// An error occurred while building a `PackageGraph`.
    #[error("error building package graph from `{path}` in archive")]
    PackageGraphConstructError {
        /// The name of the metadata file.
        path: &'static Utf8Path,

        /// The error.
        #[source]
        error: guppy::Error,
    },
}

/// An error occurred while extracting a file.
///
/// Returned by [`extract_archive`](crate::reuse_build::ReuseBuildInfo::extract_archive).
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ArchiveExtractError {
    /// An error occurred while creating a temporary directory.
    #[error("error creating temporary directory")]
    TempDirCreate(#[source] std::io::Error),

    /// An error occurred while canonicalizing the destination directory.
    #[error("error canonicalizing destination directory `{dir}`")]
    DestDirCanonicalization {
        /// The directory that failed to canonicalize.
        dir: Utf8PathBuf,

        /// The error that occurred.
        #[source]
        error: std::io::Error,
    },

    /// The destination already exists and `--overwrite` was not passed in.
    #[error("destination `{0}` already exists")]
    DestinationExists(Utf8PathBuf),

    /// An error occurred while reading the archive.
    #[error("error reading archive")]
    Read(#[source] ArchiveReadError),

    /// An error occurred while writing out a file to the destination directory.
    #[error("error writing file `{path}` to disk")]
    WriteFile {
        /// The path that we couldn't write out.
        path: Utf8PathBuf,

        /// The error that occurred.
        #[source]
        error: std::io::Error,
    },

    /// An error occurred while reporting the extraction status.
    #[error("error reporting extract status")]
    ReporterIo(std::io::Error),
}

/// An error that occurs while writing an event.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum WriteEventError {
    /// An error occurred while writing the event to the provided output.
    #[error("error writing to output")]
    Io(#[source] std::io::Error),

    /// An error occurred while operating on the file system.
    #[error("error operating on path {file}")]
    Fs {
        /// The file being operated on.
        file: Utf8PathBuf,

        /// The underlying IO error.
        #[source]
        error: std::io::Error,
    },

    /// An error occurred while producing JUnit XML.
    #[error("error writing JUnit output to {file}")]
    Junit {
        /// The output file.
        file: Utf8PathBuf,

        /// The underlying error.
        #[source]
        error: quick_junit::SerializeError,
    },
}

/// An error occurred while constructing a [`CargoConfigs`](crate::cargo_config::CargoConfigs)
/// instance.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum CargoConfigError {
    /// Failed to retrieve the current directory.
    #[error("failed to retrieve current directory")]
    GetCurrentDir(#[source] std::io::Error),

    /// The current directory was invalid UTF-8.
    #[error("current directory is invalid UTF-8")]
    CurrentDirInvalidUtf8(#[source] FromPathBufError),

    /// Parsing a CLI config option failed.
    #[error("failed to parse --config argument `{config_str}` as TOML")]
    CliConfigParseError {
        /// The CLI config option.
        config_str: String,

        /// The error that occurred trying to parse the config.
        #[source]
        error: toml_edit::TomlError,
    },

    /// Deserializing a CLI config option into domain types failed.
    #[error("failed to deserialize --config argument `{config_str}` as TOML")]
    CliConfigDeError {
        /// The CLI config option.
        config_str: String,

        /// The error that occurred trying to deserialize the config.
        #[source]
        error: toml_edit::easy::de::Error,
    },

    /// A CLI config option is not in the dotted key format.
    #[error(
        "invalid format for --config argument `{config_str}` (should be a dotted key expression)"
    )]
    InvalidCliConfig {
        /// The CLI config option.
        config_str: String,

        /// The reason why this Cargo CLI config is invalid.
        #[source]
        reason: InvalidCargoCliConfigReason,
    },

    /// A non-UTF-8 path was encountered.
    #[error("non-UTF-8 path encountered")]
    NonUtf8Path(#[source] FromPathBufError),

    /// Failed to retrieve the Cargo home directory.
    #[error("failed to retrieve the Cargo home directory")]
    GetCargoHome(#[source] std::io::Error),

    /// Failed to canonicalize a path
    #[error("failed to canonicalize path `{path}")]
    FailedPathCanonicalization {
        /// The path that failed to canonicalize
        path: Utf8PathBuf,

        /// The error the occurred during canonicalization
        #[source]
        error: std::io::Error,
    },

    /// Failed to read config file
    #[error("failed to read config at `{path}`")]
    ConfigReadError {
        /// The path of the config file
        path: Utf8PathBuf,

        /// The error that occurred trying to read the config file
        #[source]
        error: std::io::Error,
    },

    /// Failed to deserialize config file
    #[error("failed to parse config at `{path}`")]
    ConfigParseError {
        /// The path of the config file
        path: Utf8PathBuf,

        /// The error that occurred trying to deserialize the config file
        #[source]
        error: toml_edit::easy::de::Error,
    },
}

/// The reason an invalid CLI config failed.
///
/// Part of [`CargoConfigError::InvalidCliConfig`].
#[derive(Copy, Clone, Debug, Error, Eq, PartialEq)]
#[non_exhaustive]
pub enum InvalidCargoCliConfigReason {
    /// The argument is not a TOML dotted key expression.
    #[error("was not a TOML dotted key expression (such as `build.jobs = 2`)")]
    NotDottedKv,

    /// The argument includes non-whitespace decoration.
    #[error("includes non-whitespace decoration")]
    IncludesNonWhitespaceDecoration,

    /// The argument sets a value to an inline table.
    #[error("sets a value to an inline table, which is not accepted")]
    SetsValueToInlineTable,

    /// The argument sets a value to an array of tables.
    #[error("sets a value to an array of tables, which is not accepted")]
    SetsValueToArrayOfTables,

    /// The argument doesn't provide a value.
    #[error("doesn't provide a value")]
    DoesntProvideValue,
}

/// An error occurred while determining the cross-compiling target triple.
#[derive(Debug, Error)]
pub enum TargetTripleError {
    /// The environment variable contained non-utf8 content
    #[error(
        "environment variable '{}' contained non-UTF-8 data",
        TargetTriple::CARGO_BUILD_TARGET_ENV
    )]
    InvalidEnvironmentVar,

    /// Error looking up Cargo configs
    #[error("error discovering Cargo configs")]
    CargoConfigSearchError(
        #[from]
        #[source]
        CargoConfigError,
    ),
}

/// An error occurred determining the target runner
#[derive(Debug, Error)]
pub enum TargetRunnerError {
    /// Failed to determine the host triple, which is needed to determine the
    /// default target triple when a target is not explicitly specified
    #[error("unable to determine host platform")]
    UnknownHostPlatform(#[source] target_spec::Error),

    /// An environment variable contained non-utf8 content
    #[error("environment variable '{0}' contained non-UTF-8 data")]
    InvalidEnvironmentVar(String),

    /// An environment variable or config key was found that matches the target
    /// triple, but it didn't actually contain a binary
    #[error("runner '{key}' = '{value}' did not contain a runner binary")]
    BinaryNotSpecified {
        /// The source under consideration.
        key: PlatformRunnerSource,

        /// The value that was read from the key
        value: String,
    },

    /// Failed to parse the specified target triple
    #[error("failed to parse triple `{triple}`")]
    FailedToParseTargetTriple {
        /// The triple that failed to parse
        triple: String,

        /// The error that occurred parsing the triple
        #[source]
        error: target_spec::errors::TripleParseError,
    },

    /// Error looking up Cargo configs.
    #[error("error discovering Cargo configs")]
    CargoConfigSearchError(
        #[from]
        #[source]
        CargoConfigError,
    ),
}

/// An error that occurred while setting up the signal handler.
#[derive(Debug, Error)]
#[error("error setting up signal handler")]
pub struct SignalHandlerSetupError(#[from] std::io::Error);

#[cfg(feature = "self-update")]
mod self_update_errors {
    use super::*;
    use mukti_metadata::ReleaseStatus;
    use semver::{Version, VersionReq};
    use std::collections::BTreeSet;

    /// An error that occurs while performing a self-update.
    ///
    /// Returned by methods in the [`update`](crate::update) module.
    #[cfg(feature = "self-update")]
    #[derive(Debug, Error)]
    #[non_exhaustive]
    pub enum UpdateError {
        /// An error was generated by `self_update`.
        #[error("self-update failed")]
        SelfUpdate(#[source] self_update::errors::Error),

        /// Deserializing release metadata failed.
        #[error("deserializing release metadata failed")]
        ReleaseMetadataDe(#[source] serde_json::Error),

        /// This version was not found.
        #[error("version `{version}` not found (known versions: {})", known_versions(.known))]
        VersionNotFound {
            /// The version that wasn't found.
            version: Version,

            /// A list of all known versions.
            known: Vec<(Version, ReleaseStatus)>,
        },

        /// No version was found matching a requirement.
        #[error("no version found matching requirement `{req}`")]
        NoMatchForVersionReq {
            /// The version requirement that had no matches.
            req: VersionReq,
        },

        /// The specified mukti project was not found.
        #[error("project {not_found} not found in release metadata (known projects: {})", known.join(", "))]
        MuktiProjectNotFound {
            /// The project that was not found.
            not_found: String,

            /// Known projects.
            known: Vec<String>,
        },

        /// No release information was found for the given target triple.
        #[error(
            "for version {version}, no release information found for target `{triple}` \
            (known targets: {})",
            known_triples.iter().join(", ")
        )]
        NoTargetData {
            /// The version that was fetched.
            version: Version,

            /// The target triple.
            triple: String,

            /// The triples that were found.
            known_triples: BTreeSet<String>,
        },

        /// The current executable could not be determined.
        #[error("the current executable's path could not be determined")]
        CurrentExe(#[source] std::io::Error),

        /// A temporary directory could not be created.
        #[error("temporary directory could not be created at `{location}`")]
        TempDirCreate {
            /// The location where the temporary directory could not be created.
            location: Utf8PathBuf,

            /// The error that occurred.
            #[source]
            error: std::io::Error,
        },

        /// The temporary archive could not be created.
        #[error("temporary archive could not be created at `{archive_path}`")]
        TempArchiveCreate {
            /// The archive file that couldn't be created.
            archive_path: Utf8PathBuf,

            /// The error that occurred.
            #[source]
            error: std::io::Error,
        },

        /// An error occurred while writing to a temporary archive.
        #[error("error writing to temporary archive at `{archive_path}`")]
        TempArchiveWrite {
            /// The archive path for which there was an error.
            archive_path: Utf8PathBuf,

            /// The error that occurred.
            #[source]
            error: std::io::Error,
        },

        /// An error occurred while renaming a file.
        #[error("error renaming `{source}` to `{dest}`")]
        FsRename {
            /// The rename source.
            source: Utf8PathBuf,

            /// The rename destination.
            dest: Utf8PathBuf,

            /// The error that occurred.
            #[source]
            error: std::io::Error,
        },
    }

    fn known_versions(versions: &[(Version, ReleaseStatus)]) -> String {
        use std::fmt::Write;

        // Take the first few versions here.
        const DISPLAY_COUNT: usize = 4;

        let display_versions: Vec<_> = versions
            .iter()
            .filter_map(|(v, status)| {
                (v.pre.is_empty() && *status == ReleaseStatus::Active).then(|| v.to_string())
            })
            .take(DISPLAY_COUNT)
            .collect();
        let mut display_str = display_versions.join(", ");
        if versions.len() > display_versions.len() {
            write!(
                display_str,
                " and {} others",
                versions.len() - display_versions.len()
            )
            .unwrap();
        }

        display_str
    }

    #[cfg(feature = "self-update")]
    /// An error occurred while parsing an [`UpdateVersion`](crate::update::UpdateVersion).
    #[derive(Debug, Error)]
    pub enum UpdateVersionParseError {
        /// The version string is empty.
        #[error("version string is empty")]
        EmptyString,

        /// The input is not a valid version requirement.
        #[error("`{input}` is not a valid semver requirement\n\
                (hint: see https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html for the correct format)"
                )]
        InvalidVersionReq {
            /// The input that was provided.
            input: String,

            /// The error.
            #[source]
            error: semver::Error,
        },

        /// The version is not a valid semver.
        #[error("`{input}` is not a valid semver{}", extra_semver_output(.input))]
        InvalidVersion {
            /// The input that was provided.
            input: String,

            /// The error.
            #[source]
            error: semver::Error,
        },
    }

    fn extra_semver_output(input: &str) -> String {
        // If it is not a valid version but it is a valid version
        // requirement, add a note to the warning
        if input.parse::<VersionReq>().is_ok() {
            format!(
                "\n(if you want to specify a semver range, add an explicit qualifier, like ^{input})"
            )
        } else {
            "".to_owned()
        }
    }
}

#[cfg(feature = "self-update")]
pub use self_update_errors::*;
