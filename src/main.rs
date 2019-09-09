extern crate clap;
extern crate filesystem;
extern crate multimap;
extern crate sqlite;

use clap::{Arg, App, SubCommand};

use std::thread::{self, JoinHandle};

use filesystem::{FileSystem, OsFileSystem};
use std::sync::mpsc::{Sender, Receiver};
use std::sync::mpsc;
use multimap::MultiMap;

mod file;
mod rule;
mod ticket;
mod work;
mod memory;
mod station;
mod executor;
mod packet;
mod metadata;

use self::rule::Record;
use self::packet::Packet;
use self::work::{WorkResult, WorkError, OsExecutor, do_command};
use self::metadata::{MetadataGetter, OsMetadataGetter};
use self::station::{Station, TargetFileInfo};
use self::memory::Memory;

fn spawn_command<
    FileSystemType: FileSystem + Send + 'static,
    MetadataGetterType: MetadataGetter + Send + 'static>
(
    station : Station<FileSystemType, MetadataGetterType>,
    senders : Vec<(usize, Sender<Packet>)>,
    receivers : Vec<Receiver<Packet>>,
) -> JoinHandle<Result<WorkResult, WorkError>>
{
    thread::spawn(
        move || -> Result<WorkResult, WorkError>
        {
            do_command(
                station,
                senders,
                receivers,
                OsExecutor::new()
            )
        }
    )
}

fn make_multimaps(records : &Vec<Record>)
    -> (
        MultiMap<usize, (usize, Sender<Packet>)>,
        MultiMap<usize, (Receiver<Packet>)>
    )
{
    let mut senders : MultiMap<usize, (usize, Sender<Packet>)> = MultiMap::new();
    let mut receivers : MultiMap<usize, (Receiver<Packet>)> = MultiMap::new();

    for (target_index, record) in records.iter().enumerate()
    {
        for (source_index, sub_index) in record.source_indices.iter()
        {
            let (sender, receiver) : (Sender<Packet>, Receiver<Packet>) = mpsc::channel();
            senders.insert(*source_index, (*sub_index, sender));
            receivers.insert(target_index, receiver);
        }
    }

    (senders, receivers)
}

fn build(memoryfile: &str, rulefile: &str, target: &str)
{
    let mut os_file_system = OsFileSystem::new();

    match Memory::from_file(&mut os_file_system, memoryfile)
    {
        Err(why) => eprintln!("ERROR: {}", why),
        Ok(mut memory) =>
        {
            match file::read(rulefile)
            {
                Err(why) => eprintln!("ERROR: {}", why),
                Ok(content) =>
                {
                    match rule::parse(content)
                    {
                        Err(why) => eprintln!("ERROR: {}", why),
                        Ok(rules) =>
                        {
                            match rule::topological_sort(rules, &target)
                            {
                                Err(why) => eprintln!("ERROR: {}", why),
                                Ok(mut records) =>
                                {
                                    let (mut senders, mut receivers) = make_multimaps(&records);

                                    let mut handles = Vec::new();
                                    let mut index : usize = 0;

                                    for mut record in records.drain(..)
                                    {
                                        let sender_vec = match senders.remove(&index)
                                        {
                                            Some(v) => v,
                                            None => vec![],
                                        };

                                        let receiver_vec = match receivers.remove(&index)
                                        {
                                            Some(v) => v,
                                            None => vec![],
                                        };

                                        let mut target_infos = Vec::new();
                                        for target_path in record.targets.drain(..)
                                        {
                                            target_infos.push(
                                                TargetFileInfo
                                                {
                                                    history : memory.get_target_history(&target_path),
                                                    path : target_path,
                                                }
                                            );
                                        }

                                        let station = Station::new(
                                            target_infos,
                                            record.command,
                                            memory.get_rule_history(&record.ticket),
                                            os_file_system.clone(),
                                            OsMetadataGetter::new(),
                                        );

                                        handles.push(
                                            (
                                                record.ticket,
                                                spawn_command(
                                                    station,
                                                    sender_vec,
                                                    receiver_vec,
                                                )
                                            )
                                        );

                                        index+=1;
                                    }

                                    for (record_ticket, handle) in handles
                                    {
                                        match handle.join()
                                        {
                                            Err(_) =>
                                            {
                                                eprintln!("ERROR");
                                            },
                                            Ok(work_result_result) =>
                                            {
                                                match work_result_result
                                                {
                                                    Ok(work_result) =>
                                                    {
                                                        match work_result.command_line_output
                                                        {
                                                            Some(output) =>
                                                            {
                                                                for target_info in work_result.target_infos.iter()
                                                                {
                                                                    println!("{}", target_info.path);
                                                                }

                                                                if output.out != ""
                                                                {
                                                                    println!("output: {}", output.out);
                                                                }

                                                                if output.err != ""
                                                                {
                                                                    println!("error: {}", output.err);
                                                                }

                                                                if !output.success
                                                                {
                                                                    println!("success: {}", output.success);
                                                                    println!("code: {}", 
                                                                        match output.code
                                                                        {
                                                                            Some(code) => format!("{}", code),
                                                                            None => "None".to_string(),
                                                                        }
                                                                    );
                                                                }

                                                            },
                                                            None => {},
                                                        }

                                                        memory.insert_rule_history(record_ticket, work_result.rule_history);
                                                    },
                                                    Err(why) =>
                                                    {
                                                        eprintln!("ERROR {}", why);
                                                    },
                                                }
                                            }
                                        }
                                    }

                                    match memory.to_file(&mut os_file_system, memoryfile)
                                    {
                                        Ok(_) => println!("done"),
                                        Err(_) => eprintln!("Error writing history"),
                                    }
                                },
                            }
                        }
                    }
                },
            }
        }
    }
}


fn main()
{
    let big_matches = App::new("rules")
        .version("0.1.0")
        .author("Peterson Trethewey <ptrethewey@roblox.com>")
        .about("You know when you have files that depend on other files?  This is for that situation.")
        .subcommand(
            SubCommand::with_name("clean")
            .help("Removes all files and directories specificed as targets in the rulefile"))
        .subcommand(
            SubCommand::with_name("build")
            .help("Builds the given target")
            .arg(Arg::with_name("target")
                .help("The path to the target file (or directory) to be built")
                .required(true)
                .index(1)))
        .arg(Arg::from_usage("-r --rules=[RULES] 'Sets a rule file to use'"))
        .arg(Arg::from_usage("-t --target=[TARGET] 'Sets which target to build'"))
        .arg(Arg::from_usage("-m --history=[HISTORY] 'Where to read/write cached file content data'"))
        .get_matches();

    if let Some(_matches) = big_matches.subcommand_matches("clean")
    {
        println!("here's where we would clean");
    }

    if let Some(matches) = big_matches.subcommand_matches("build")
    {
        let historyfile =
        match matches.value_of("history")
        {
            Some(value) => value,
            None => ".ruler-history",
        };

        let rulefile =
        match matches.value_of("rules")
        {
            Some(value) => value,
            None => "rulefile",
        };

        let target =
        match matches.value_of("target")
        {
            Some(value) => value,
            None => panic!("No target!"),
        };

        build(historyfile, rulefile, target);
    }
}
