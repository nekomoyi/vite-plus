mod common;
pub(crate) mod config;
pub(crate) mod create;
pub(crate) mod hooks;
pub(crate) mod migrate;
pub(crate) mod staged;

#[cfg(test)]
pub(crate) fn all_commands() -> Vec<clap::Command> {
    vec![
        create::create_command(),
        migrate::migrate_command(),
        config::config_command(),
        hooks::hooks_command(),
        staged::staged_command(),
    ]
}
