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

mod file;
mod rulefile;
mod hash;

use self::rulefile::Rule;
use self::hash::{Hash, HashFactory};

fn run_command(
    rule: &Rule,
    senders : Vec<Sender<Hash>>,
    receivers : Vec<Receiver<Hash>>,
    protected_connection : Arc<Mutex<Connection>> ) -> JoinHandle<Output>
{
    if rule.command.len() == 0
    {
        let mut command = Command::new("echo");
        command.arg(format!("hello {}", rule.targets[0]));

        thread::spawn(
            move || -> Output
            {
                let out = command.output().expect("failed to execute process");

                for snd in senders
                {
                    snd.send(HashFactory::new_from_str("").result());
                }

                out
            }
        )
    }
    else
    {
        let command_vec = &rule.command;
        let mut command = Command::new(command_vec[0]);
        for argument in command_vec[1..].iter()
        {
            command.arg(argument);
        }

        let mut factory = HashFactory::new_from_str(rule.all);

        thread::spawn(
            move || -> Output
            {
                for rcv in receivers
                {
                    factory.input_hash( rcv.recv().unwrap() );
                }

                let out = command.output().expect("failed to execute process");

                for snd in senders
                {
                    snd.send(HashFactory::new_from_str("").result());
                }

                out
            }
        )
    }
}

use sqlite::{Connection, State};
use std::sync::{Arc, Mutex};


fn get_target_hash(connection : &Connection, sha_str : &str) -> String
{
    let mut statement = connection
        .prepare("SELECT * FROM history WHERE source = ?")
        .unwrap();

    statement.bind(1, sha_str).unwrap();

    match statement.next().unwrap()
    {
        State::Row => statement.read::<String>(1).unwrap(),
        _ => String::new(),
    }
}

fn main()
{
    let matches = App::new("build")
        .version("0.1.0")
        .author("Peterson Trethewey <ptrethewey@roblox.com>")
        .about("You know when you have files that depend on other files?  This is for that situation.")
        .arg(Arg::with_name("BUILDFILE")
             .required(true)
             .takes_value(true)
             .index(1)
             .help("path to build rules file"))
        .arg(Arg::from_usage("-t --target=[TARGET] 'Sets which target to build'"))
        .get_matches();

    let buildfile = matches.value_of("BUILDFILE").unwrap();

    match matches.value_of("target")
    {
        Some(target) =>
        {
            println!("Reading rulesfile: {}", buildfile);
            println!("Building target: {}", target);

            match file::read(&buildfile.to_string())
            {
                Err(why) => eprintln!("ERROR: {}", why),
                Ok(content) => 
                {
                    match rulefile::parse(&content)
                    {
                        Err(why) => eprintln!("ERROR: {}", why),
                        Ok(rules) =>
                        {
                            let (rules_in_order, source_to_index) =
                                rulefile::topological_sort(rules, &target);

                            let mut senders = MultiMap::new();
                            let mut receivers = MultiMap::new();

                            for (target_index, rule) in rules_in_order.iter().enumerate()
                            {
                                for source in rule.sources.iter()
                                {
                                    let (sender, receiver) : (Sender<Hash>, Receiver<Hash>) = mpsc::channel();

                                    println!("source: {}", source);
                                    let source_index = source_to_index.get(*source).unwrap();
                                    println!("index: {}", source_index);

                                    senders.insert(source_index, sender);
                                    receivers.insert(target_index, receiver);
                                }
                            }

                            let connection = sqlite::open("history.db").unwrap();
                            connection.execute(
                                "CREATE TABLE IF NOT EXISTS history (source varchar(88), target varchar(88), UNIQUE(source) );"
                            ).unwrap();

                            let protected_connection = Arc::new(Mutex::new(connection));

                            let mut handles = Vec::new();
                            for (index, rule) in rules_in_order.iter().enumerate()
                            {
                                println!("{} : {}", index, rule);

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
                                        &rule,
                                        sender_vec,
                                        receiver_vec,
                                        protected_connection.clone()
                                    )
                                );
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
