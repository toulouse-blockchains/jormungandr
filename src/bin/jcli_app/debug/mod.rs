mod message;

use cardano::util::hex;
use jcli_app::utils::error::CustomErrorFiller;
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(StructOpt)]
#[structopt(rename_all = "kebab-case")]
pub enum Debug {
    /// Decode hex-encoded message an display its content
    Message(message::Message),
}

custom_error! {pub Error
    Io { source: std::io::Error } = "I/O Error",
    InputInvalid { source: std::io::Error, path: PathBuf }
        = @{{ let _ = source; format_args!("Invalid input file path '{}'", path.display()) }},
    HexMalformed { source: hex::Error } = "Hex encoding malformed",
    MessageMalformed { source: std::io::Error, filler: CustomErrorFiller } = "Message malformed",
}

impl Debug {
    pub fn exec(self) -> Result<(), Error> {
        match self {
            Debug::Message(message) => message.exec(),
        }
    }
}
