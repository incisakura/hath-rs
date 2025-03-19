
/// /t/$testsize/$testtime/$testkey
mod speedtest;
pub(crate) use speedtest::speed_test;
pub(crate) use speedtest::SpeedTest;

/// /h/$fileid/$additional/$filename
mod file_fetch;
pub(crate) use file_fetch::file_fetch;

/// /servercmd/$command/$additional/$time/$key
mod server_command;
pub(crate) use server_command::server_command;
