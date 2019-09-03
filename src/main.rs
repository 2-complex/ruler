extern crate clap;
extern crate sqlite;
extern crate multimap;
extern crate filesystem;

use clap::{Arg, App};

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
use self::executor::CommandLineOutput;
use self::work::{OsExecutor, do_command};
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
) -> JoinHandle<Result<CommandLineOutput, String>>
{
    thread::spawn(
        move || -> Result<CommandLineOutput, String>
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

fn main()
{
    let matches = App::new("build")
        .version("0.1.0")
        .author("Peterson Trethewey <ptrethewey@roblox.com>")
        .about("You know when you have files that depend on other files?  This is for that situation.")
        .arg(Arg::with_name("COMMAND")
             .required(true)
             .takes_value(true)
             .index(1)
             .help("path to build rules file"))
        .arg(Arg::from_usage("-r --rules=[RULES] 'Sets a rule file to use'"))
        .arg(Arg::from_usage("-t --target=[TARGET] 'Sets which target to build'"))
        .arg(Arg::from_usage("-m --memory=[MEMORY] 'Where to read/write cached file content data'"))
        .get_matches();

    let mut os_file_system = OsFileSystem::new();

    let memoryfile =
    match matches.value_of("memory")
    {
        Some(value) => value,
        None => "my.memory",
    };

    let rulefile =
    match matches.value_of("rules")
    {
        Some(value) => value,
        None => panic!("No rules!"),
    };

    match matches.value_of("target")
    {
        None =>
        {
            eprintln!("ERROR no target to build");
        },
        Some(target) =>
        {
            println!("Reading rulefile: {}", rulefile);
            println!("Reading memory file: {}", memoryfile);
            println!("Building target: {}", target);

            match Memory::from_file(&mut os_file_system, memoryfile)
            {
                Err(why) => eprintln!("ERROR: {}", why),
                Ok(mut memory) =>
                {
                    match file::read(&rulefile.to_string())
                    {
                        Err(why) => eprintln!("ERROR: {}", why),
                        Ok(content) =>
                        {
                            match rule::parse(content)
                            {
                                Err(why) => eprintln!("ERROR: {}", why),
                                Ok(rules) =>
                                {
                                    let os_file_system = OsFileSystem::new();
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
                                                    spawn_command(
                                                        station,
                                                        sender_vec,
                                                        receiver_vec,
                                                    )
                                                );

                                                index+=1;
                                            }

                                            for handle in handles
                                            {
                                                match handle.join()
                                                {
                                                    Err(_) =>
                                                    {
                                                        eprintln!("ERROR");
                                                    },
                                                    Ok(command_result) =>
                                                    {
                                                        match command_result
                                                        {
                                                            Ok(r) =>
                                                            {
                                                                println!("success: {}", r.success);
                                                                println!("code: {}", match r.code
                                                                {
                                                                    Some(code) => format!("{}", code),
                                                                    None => "None".to_string(),
                                                                });

                                                                println!("output: {}", r.out);
                                                                println!("error: {}", r.err);
                                                            },
                                                            Err(why) =>
                                                            {
                                                                eprintln!("ERROR {}", why);
                                                            },
                                                        }
                                                    }
                                                }
                                            }
                                        },
                                    }
                                }
                            }
                        },
                    }

                }
            }

        },
    };
}
