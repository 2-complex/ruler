extern crate clap;
extern crate crypto;
extern crate sqlite;
extern crate multimap;

use clap::{Arg, App};
use crypto::sha2::Sha512;
use crypto::digest::Digest;

use std::process::{Output, Command};
use std::thread::{self, JoinHandle};

use base64::{encode};
use std::io::{self, Write};

use std::sync::mpsc::{Sender, Receiver};
use std::sync::mpsc;
use multimap::MultiMap;

mod file;
mod rulefile;

use self::rulefile::Rule;

fn sha_512(input: &str, out: &mut [u8])
{
    let mut dig = Sha512::new();
    dig.input(input.as_bytes());
    dig.result(out);
}

fn base64_sha(sha: &[u8]) -> String
{
    format!("{}", encode(&sha))
}

fn base64_sha_str(content: &str) -> String
{
    let mut sha : [u8; 64] = [0; 64];
    sha_512(&content, &mut sha);
    base64_sha(&sha)
}


fn spawn_command(
    command_first : &str,
    command_args : Vec<&str>,
    senders : Vec<Sender<i32>>,
    receivers : Vec<Receiver<i32>>) -> JoinHandle<Output>
{
    let mut command = Command::new(command_first);
    for argument in command_args
    {
        command.arg(argument);
    }

    thread::spawn(
        move || -> Output
        {
            for rcv in receivers
            {
                rcv.recv();
            }

            let out = command.output().expect("failed to execute process");

            for snd in senders
            {
                snd.send(1);
            }

            out
        }
    )
}

fn run_command(rule: &Rule,
    senders : Vec<Sender<i32>>,
    receivers : Vec<Receiver<i32>>) -> JoinHandle<Output>
{
    let command = &rule.command;
    spawn_command(command[0], command[1..].to_vec(), senders, receivers)
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
                            let deps_in_order = rulefile::topological_sort_indices(&rules, &target);

                            let mut senders = MultiMap::new();
                            let mut receivers = MultiMap::new();

                            for target_index in deps_in_order.iter()
                            {
                                for source_index in rules[*target_index].source_rule_indices.iter()
                                {
                                    let (sender, receiver) : (Sender<i32>, Receiver<i32>) = mpsc::channel();
                                    senders.insert(source_index, sender);
                                    receivers.insert(target_index, receiver);
                                }
                            }

                            let mut handles = Vec::new();
                            for i in deps_in_order.iter()
                            {
                                let sender_vec = match senders.remove(i)
                                {
                                    Some(v) => v,
                                    None => Vec::new(),
                                };

                                let receiver_vec = match receivers.remove(i)
                                {
                                    Some(v) => v,
                                    None => Vec::new(),
                                };

                                handles.push(
                                    run_command(
                                        &rules[*i], sender_vec, receiver_vec));
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


    let connection = sqlite::open("history.db").unwrap();
    connection.execute(
        "CREATE TABLE IF NOT EXISTS history (source varchar(88), target varchar(88), UNIQUE(source) );"
    ).unwrap();

    let protected_connection = Arc::new(Mutex::new(connection));

    println!( "getting target hash2: {}", get_target_hash(&protected_connection.lock().unwrap(), &base64_sha_str("stuff2")) );
    println!( "getting target hash: {}", get_target_hash(&protected_connection.lock().unwrap(), &base64_sha_str("stuff")) );
}
