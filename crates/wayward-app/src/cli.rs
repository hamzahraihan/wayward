#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliAction {
    Run,
    PrintConfigPath,
    OpenConfig,
    SetCredential { provider: String },
    DeleteCredential { provider: String },
}

/// Parse CLI arguments. The first recognized flag in argument order wins;
/// unknown flags are ignored. `--set-credential` consumes the next argument
/// as the provider id.
pub fn classify<I, S>(args: I) -> CliAction
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let args: Vec<_> = args
        .into_iter()
        .map(|arg| arg.as_ref().to_os_string())
        .collect();

    for (index, arg) in args.iter().enumerate() {
        if arg == std::ffi::OsStr::new("--config-path") {
            return CliAction::PrintConfigPath;
        }
        if arg == std::ffi::OsStr::new("--open-config") {
            return CliAction::OpenConfig;
        }
        if arg == std::ffi::OsStr::new("--set-credential") {
            let provider = args
                .get(index + 1)
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_owned();
            return CliAction::SetCredential { provider };
        }
        if arg == std::ffi::OsStr::new("--delete-credential") {
            let provider = args
                .get(index + 1)
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_owned();
            return CliAction::DeleteCredential { provider };
        }
    }
    CliAction::Run
}

pub fn parse_args() -> CliAction {
    classify(std::env::args_os().skip(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn empty_args_run() {
        assert_eq!(classify(Vec::<OsString>::new()), CliAction::Run);
    }

    #[test]
    fn config_path_flag_prints_path() {
        assert_eq!(
            classify(args(&["--config-path"])),
            CliAction::PrintConfigPath
        );
    }

    #[test]
    fn open_config_flag_opens_config() {
        assert_eq!(classify(args(&["--open-config"])), CliAction::OpenConfig);
    }

    #[test]
    fn set_credential_consumes_provider_argument() {
        assert_eq!(
            classify(args(&["--set-credential", "claude-official"])),
            CliAction::SetCredential {
                provider: "claude-official".to_owned()
            }
        );
    }

    #[test]
    fn set_credential_without_provider_yields_empty_id() {
        assert_eq!(
            classify(args(&["--set-credential"])),
            CliAction::SetCredential {
                provider: String::new()
            }
        );
    }

    #[test]
    fn delete_credential_consumes_provider_argument() {
        assert_eq!(
            classify(args(&["--delete-credential", "codex-official"])),
            CliAction::DeleteCredential {
                provider: "codex-official".to_owned()
            }
        );
    }

    #[test]
    fn unknown_flag_mixed_with_known_resolves_to_known() {
        assert_eq!(
            classify(args(&["--bogus", "--config-path"])),
            CliAction::PrintConfigPath
        );
    }

    #[test]
    fn first_of_two_recognized_flags_wins() {
        assert_eq!(
            classify(args(&["--open-config", "--config-path"])),
            CliAction::OpenConfig
        );
        assert_eq!(
            classify(args(&["--config-path", "--open-config"])),
            CliAction::PrintConfigPath
        );
    }
}
