extern crate clap;
extern crate filesystem;

use clap::{Arg, App, SubCommand};
use filesystem::OsFileSystem;

mod cache;
mod build;
mod rule;
mod ticket;
mod work;
mod memory;
mod executor;
mod packet;
mod metadata;

use self::executor::OsExecutor;
use self::metadata::OsMetadataGetter;

fn main()
{
    let big_matches = App::new("Ruler")
        .version("0.1.0")
        .author("Peterson Trethewey <peterson@2-complex.com>")
        .about("
Ruler is a tool for managing a dependence graph of files.  It works with a
.rules file.  A .rules file contains newline-separated blocks called 'rules'.
Each rule looks like this:

path/to/target/file1
path/to/target/file2
:
path/to/source/file1
path/to/source/file2
path/to/source/file3
:
command
--option1=value1
--option2=value2
:

The command-line invocation is meant to update the target files using the
source files as input.

Ruler maintains a history of file-hashes to determine whether a target needs to
update.  When you type a build command such as:

ruler build

Ruler checks the current state of the targets against the hashes it has on
file, determines which ones need to update and in what order, and runs the
commands for those rules.

")
        .setting(clap::AppSettings::ArgRequiredElseHelp)
        .subcommand(
            SubCommand::with_name("clean")
            .help("
Removes all files and directories specificed as targets in the rules file.")
            .arg(Arg::with_name("target")
                .help("
The path to the clean-target.  The clean command removes all files listed as
targets in rules that the clean-target depends on.  If no clean-target is
specified, the clean command removes all files listed as targets in any rule.
")
                .required(false)
                .index(1)
            )
        )
        .subcommand(
            SubCommand::with_name("build")
            .help("
Builds the given target.  If no build-target is specified, builds all targets.
The target must be a file listed in the target section of the current rules file.
The rules file is either a file in the current working directory called \"build.rules\"
or it can be specificed using --rules=<path>
")
            .arg(Arg::with_name("target")
                .help("
The path to the build-target.")
                .required(false)
                .index(1)
            )
        )
        .get_matches();


    if let Some(matches) = big_matches.subcommand_matches("clean")
    {
        let rulefile =
        match matches.value_of("rules")
        {
            Some(value) => value,
            None => "build.rules",
        };

        let directory =
        match matches.value_of("directory")
        {
            Some(value) => value,
            None => ".ruler",
        };

        let target =
        match matches.value_of("target")
        {
            Some(value) => Some(value.to_string()),
            None => None,
        };

        match build::clean(
            OsFileSystem::new(),
            OsMetadataGetter::new(),
            directory, rulefile, target)
        {
            Ok(()) => {},
            Err(error) => eprintln!("{}", error),
        }
    }

    if let Some(matches) = big_matches.subcommand_matches("build")
    {
        let rulefile =
        match matches.value_of("rules")
        {
            Some(value) => value,
            None => "build.rules",
        };

        let directory =
        match matches.value_of("directory")
        {
            Some(value) => value,
            None => ".ruler",
        };

        let target =
        match matches.value_of("target")
        {
            Some(value) => Some(value.to_string()),
            None => None,
        };

        match build::build(
            OsFileSystem::new(),
            OsExecutor::new(),
            OsMetadataGetter::new(),
            directory,
            rulefile,
            target)
        {
            Ok(()) => {},
            Err(error) => eprintln!("{}", error),
        }
    }
}
