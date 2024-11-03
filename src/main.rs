extern crate clap;
extern crate clap_derive;
extern crate toml;
extern crate serde;
extern crate execute;

use clap::Parser;
use clap_derive::
{
    Parser,
    Subcommand,
};
use crate::system::real::RealSystem;
use crate::printer::StandardPrinter;
use crate::ticket::TicketFactory;

mod blob;
mod bundle;
mod build;
mod cache;
mod directory;
mod current;
mod history;
mod packet;
mod printer;
mod rule;
mod server;
mod sort;
mod system;
mod ticket;
mod work;
mod downloader;

#[derive(Parser)]
struct BuildConfig
{
    #[arg(index=1, value_name = "TARGET_PATH", help =
"When specified, Ruler searches for a dependnece rule in which TARGET_PATH is
listed as a target, and limit build/clean operations to that rule and its
ancestors.")]
    target : Option<String>,
}

#[derive(Parser)]
struct RunConfig
{
    #[arg(index=1, required=true, value_name = "EXECUTABLE", help =
"A path to the executable to build and run.")]
    executable : String,

    #[arg(index=2, help=
"Arguments forwarded to the executable when it runs.")]
    extra_args: Vec<String>,
}

#[derive(Parser)]
struct ServeConfig
{
    #[arg(index=1, value_name = "PORT", default_value="build.rules", help = "An HTTP port number on which to serve")]
    port : u16,
}

#[derive(Parser)]
struct ListConfig
{
    #[arg(index=1, value_name = "PATH", help = "A path")]
    path : String,
}

#[derive(Parser)]
struct HashConfig
{
    #[arg(index=1, value_name = "PATH", help = "A path")]
    path : String,
}

#[derive(Subcommand)]
enum RulerSubcommand
{
    #[command(about="Builds the given target or all targets", long_about=
"Builds the given target.  If no build-target is specified, builds all targets.
The target must be a file listed in the target section of the current rules
file.")]
    Build(BuildConfig),

    #[command(about="Builds and runs an executable", long_about=
"The run subcommand builds EXECUTABLE as it would any target.  Provided the
build succeeds, Ruler then invokes the executable passing EXTRA_ARGS as
command-line arguments.")]
    Run(RunConfig),

    #[command(about="Caches all targets", long_about =
"Removes all files and directories specificed as targets in the rules file.
If a target is specified, removes all that target's ancestors.

Note: clean does not delete the files, it moves them to a cache so they can be
recovered later if needed.

If a target is specified, cleans only the ancestors of that target.")]
    Clean(BuildConfig),

    #[command(about="Run a server", long_about =
"Starts a server which provides cached files to other computers on the network")]
    Serve(ServeConfig),

    #[command(about="List directory", long_about =
"Kinda like ls or dir, this is a temporary feature for use in testing the interanl library's feature")]
    List(ListConfig),

    #[command(about="Hash a file or directory", long_about =
"Takes a filesystem path and returns the hash of the file or directory at that path.")]
    Hash(HashConfig),
}


#[derive(Parser)]
#[command(version = "1.1.6",
    about = "https://rulerbuild.com",
    long_about = "A straight-forward, general-purpose build tool.\nhttps://rulerbuild.com")]
struct CommandLineParser
{
    #[command(subcommand)]
    command: RulerSubcommand,

    #[arg(short, long, default_value="build.rules", value_name = "RULES_FILE", help =
"A .rules file defining the dependence graph for build, run and clean operations")]
    rules : Vec<String>,

    #[arg(short, long, default_value=".ruler", help =
"Ruler uses this directory to store cached files, rule history and information
about the current filesystem state.")]
    directory : String,
}

use crate::system::System;


fn main()
{
    let command_line = CommandLineParser::parse();

    match command_line.command
    {
        RulerSubcommand::Build(build_config) =>
        {
            match build::build(
                RealSystem::new(),
                &mut StandardPrinter::new(),
                build::BuildParams::from_all(
                    command_line.directory,
                    command_line.rules,
                    None,
                    build_config.target
                ))
            {
                Ok(()) => {},
                Err(error) => eprintln!("{}", error),
            }
        },
        RulerSubcommand::Run(run_config) =>
        {
            match build::run(
                RealSystem::new(),
                &command_line.directory,
                command_line.rules,
                None,
                run_config.executable,
                run_config.extra_args,
                &mut StandardPrinter::new())
            {
                Ok(()) => {},
                Err(error) => eprintln!("{}", error),
            }
        },
        RulerSubcommand::Clean(build_config) =>
        {
            match build::clean(
                RealSystem::new(),
                &command_line.directory,
                command_line.rules,
                build_config.target)
            {
                Ok(()) => {},
                Err(error) => eprintln!("{}", error),
            }
        },
        RulerSubcommand::Serve(serve_config) =>
        {
            match server::serve(
                RealSystem::new(),
                &command_line.directory,
                serve_config.port)
            {
                Ok(()) => {},
                Err(error) => eprintln!("{}", error),
            }
        },
        RulerSubcommand::List(list_config) =>
        {
            match RealSystem::new().list_dir(&list_config.path)
            {
                Ok(list) =>
                {
                    for l in list
                    {
                        println!("{}", l);
                    }
                },
                Err(error) => eprintln!("{}", error),
            }
        },
        RulerSubcommand::Hash(config) =>
        {
            match TicketFactory::from_path(&RealSystem::new(), &config.path)
            {
                Ok(mut factory) => println!("{}", factory.result().human_readable()),
                Err(error) => eprintln!("{}", error),
            }
        }
    }
}
