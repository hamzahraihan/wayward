#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliAction {
    Run,
    PrintConfigPath,
    OpenConfig,
}

pub fn classify<I, S>(args: I) -> CliAction
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    // The first recognized flag in argument order wins; unknown flags are ignored.
    for arg in args {
        let arg = arg.as_ref();
        if arg == std::ffi::OsStr::new("--config-path") {
            return CliAction::PrintConfigPath;
        }
        if arg == std::ffi::OsStr::new("--open-config") {
            return CliAction::OpenConfig;
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
