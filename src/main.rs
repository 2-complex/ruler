extern crate clap;
extern crate sqlite;
extern crate multimap;

use clap::{Arg, App};

use std::process::{Output, Command};
use std::thread::{self, JoinHandle};

use std::sync::mpsc::{Sender, Receiver};
use std::sync::mpsc;
use std::str::from_utf8;
use multimap::MultiMap;

use std::collections::VecDeque;

mod file;
mod rule;
mod hash;

use self::rule::Record;
use self::hash::{Hash, HashFactory};

struct CommandResult
{
    out : String,
    err : String,
    code : Option<i32>,
    success : bool,
}

impl CommandResult
{
    fn from_output(output: Output) -> CommandResult
    {
        CommandResult
        {
            out : match from_utf8(&output.stdout)
            {
                Ok(text) => text,
                Err(_) => "<non-utf8 data>",
            }.to_string(),

            err : match from_utf8(&output.stderr)
            {
                Ok(text) => text,
                Err(_) => "<non-utf8 data>",
            }.to_string(),

            code : output.status.code(),
            success : output.status.success(),
        }
    }

    fn new() -> CommandResult
    {
        CommandResult
        {
            out : "".to_string(),
            err : "".to_string(),
            code : Some(0),
            success : true,
        }
    }
}

fn run_command(
    record: Record,
    senders : Vec<(usize, Sender<Hash>)>,
    receivers : Vec<Receiver<Hash>> )
    -> JoinHandle<Result<CommandResult, String>>
{
    thread::spawn(
        move || -> Result<CommandResult, String>
        {
            let mut command_queue = VecDeque::from(record.command.clone());
            let mut factory = record.hash_factory;

            let command_opt = match command_queue.pop_front()
            {
                Some(first) =>
                {
                    let mut command = Command::new(first);
                    while let Some(argument) = command_queue.pop_front()
                    {
                        command.arg(argument);
                    }
                    Some(command)
                },
                None => None
            };

            for rcv in receivers
            {
                match rcv.recv()
                {
                    Ok(h) => factory.input_hash(h),
                    Err(why) =>
                    {
                        eprintln!("ERROR {}", why);
                    },
                }
            }

            let result =
            match command_opt
            {
                Some(mut command) =>
                {
                    match command.output()
                    {
                        Ok(out) => Ok(CommandResult::from_output(out)),
                        Err(why)=>
                        {
                            return Err(format!("Error in command to build: {}\n{}", record.targets.join(" "), why))
                        },
                    }
                },
                None => Ok(CommandResult::new()),
            };

            for (sub_index, sender) in senders
            {
                match HashFactory::from_filepath(&record.targets[sub_index])
                {
                    Ok(mut hash) =>
                    {
                        match sender.send(hash.result())
                        {
                            Ok(_) => {},
                            Err(_error) => eprintln!("CHANNEL SEND ERROR"),
                        }
                    },
                    Err(_error) =>
                    {
                        eprintln!("FILE IO ERROR");
                    },
                }
            }

            result
        }
    )
}

fn make_multimaps(records : &Vec<Record>)
    -> (
        MultiMap<usize, (usize, Sender<Hash>)>,
        MultiMap<usize, (Receiver<Hash>)>
    )
{
    let mut senders : MultiMap<usize, (usize, Sender<Hash>)> = MultiMap::new();
    let mut receivers : MultiMap<usize, (Receiver<Hash>)> = MultiMap::new();

    for (target_index, record) in records.iter().enumerate()
    {
        for (source_index, sub_index) in record.source_indices.iter()
        {
            let (sender, receiver) : (Sender<Hash>, Receiver<Hash>) = mpsc::channel();
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
        .arg(Arg::with_name("RULEFILE")
             .required(true)
             .takes_value(true)
             .index(1)
             .help("path to build rules file"))
        .arg(Arg::from_usage("-t --target=[TARGET] 'Sets which target to build'"))
        .get_matches();

    let rulefile = matches.value_of("RULEFILE").unwrap();

    match matches.value_of("target")
    {
        Some(target) =>
        {
            println!("Reading rulefile: {}", rulefile);
            println!("Building target: {}", target);

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
                            match rule::topological_sort(rules, &target)
                            {
                                Err(why) => eprintln!("ERROR: {}", why),
                                Ok(mut records) =>
                                {
                                    let (mut senders, mut receivers) = make_multimaps(&records);

                                    let mut handles = Vec::new();
                                    let mut index : usize = 0;

                                    for record in records.drain(..)
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

                                        handles.push(
                                            run_command(
                                                record,
                                                sender_vec,
                                                receiver_vec
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


        },
        None =>
        {
            eprintln!("ERROR no target to build");
        },
    };
}
