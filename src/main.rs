extern crate clap;
extern crate sqlite;
extern crate multimap;

use clap::{Arg, App};

use std::process::{Output, Command};
use std::thread::{self, JoinHandle};

use std::io::{self, Write};

use std::sync::mpsc::{Sender, Receiver};
use std::sync::mpsc;
use multimap::MultiMap;

use std::collections::VecDeque;

mod file;
mod rule;
mod hash;

use self::rule::Record;
use self::hash::{Hash, HashFactory};

fn run_command(
    mut record: Record,
    senders : Vec<(usize, Sender<Hash>)>,
    receivers : Vec<Receiver<Hash>>,
    _protected_connection : Arc<Mutex<Connection>> )
    -> JoinHandle<Output>
{
    let mut factory = HashFactory::new_from_str(&record.all());

    thread::spawn(
        move || -> Output
        {
            let mut command_queue = VecDeque::from(record.command);

            let mut command =
            if let Some(first) = command_queue.pop_front()
            {
                let mut command = Command::new(first);
                while let Some(argument) = command_queue.pop_front()
                {
                    command.arg(argument);
                }
                command
            }
            else
            {
                Command::new("echo hello")
            };

            for rcv in receivers
            {
                factory.input_hash( rcv.recv().unwrap() );
            }

            let out = command.output().expect("failed to execute process");

            for (sub_index, sender) in senders
            {
                match HashFactory::new_from_filepath(&record.targets[sub_index])
                {
                    Ok(mut hash) =>
                    {
                        sender.send(hash.result());
                    }
                    Err(_error) =>
                    {
                        eprintln!("FILE IO ERROR");
                    }
                }
            }

            out
        }
    )
}

use sqlite::Connection;
use std::sync::{Arc, Mutex};

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

                                    let connection = sqlite::open("history.db").unwrap();
                                    connection.execute(
                                        "CREATE TABLE IF NOT EXISTS history (source varchar(88), target varchar(88), UNIQUE(source) );"
                                    ).unwrap();

                                    let protected_connection = Arc::new(Mutex::new(connection));

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
                                                receiver_vec,
                                                protected_connection.clone()
                                            )
                                        );

                                        index+=1;
                                    }

                                    for h in handles
                                    {
                                        match h.join()
                                        {
                                            Err(_) =>
                                            {
                                                eprintln!("ERROR");
                                            },
                                            Ok(output) =>
                                            {
                                                println!("status: {}", output.status);
                                                println!("out:");
                                                io::stdout().write_all(&output.stdout).unwrap();
                                                println!("err:");
                                                io::stderr().write_all(&output.stderr).unwrap();
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
