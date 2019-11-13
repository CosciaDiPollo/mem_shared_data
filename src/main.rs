use async_std::{
    prelude::*,
    task,
    net::{TcpListener, ToSocketAddrs, TcpStream, SocketAddr},
    io::{self, BufReader},
};

use std::sync::Arc;

use futures::channel::mpsc;
use futures::SinkExt;

use std::collections::hash_map::HashMap;


type Receive<T> = mpsc::UnboundedReceiver<T>;
type Sender<T> = mpsc::UnboundedSender<T>;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Debug)]
enum Command {
    Get {
        variable_name: String,
        from: SocketAddr
    },

    Set {
        variable_name: String,
        variable_value: String,
        from: SocketAddr
    },
    
    Unknown{
        command_name: String,
        from: SocketAddr
    },

    NewClient(Arc<TcpStream>),
}


impl Command {
    pub fn new_get(variable_name: String, from: SocketAddr) -> Self {
        Self::Get{
            variable_name,
            from
        }
    }

    pub fn new_set(variable_name: String, variable_value: String, from: SocketAddr) -> Self {
        Self::Set{
            variable_name,
            variable_value,
            from
        }
    }

    pub fn new_unknown(command_name: String, from: SocketAddr) -> Self {
        Self::Unknown{
            command_name,
            from
        }
    }
}

struct CommandParser{
    source_addr: SocketAddr,
}

impl CommandParser {
    pub fn new(source_addr: SocketAddr) -> Self {
        Self {
            source_addr
        }
    }

    fn parse_command(&self, line_string: &str) -> Option<Command>{
        let mut splitted_command = line_string.split_whitespace();
        let command = if let Some(command_type) = splitted_command.next() {
                println!("Command is '{}'", command_type);
                command_type
        } else {
                println!("Could not be able to parse the command");
                ""
        };
    
        match command {
            "get" => self.parse_get_command(splitted_command),
            "set" => self.parse_set_command(splitted_command),
            _ => Command::new_unknown(command.to_owned(), self.source_addr).into(),
        }
    }

    fn parse_get_command<'a>(&self, mut splitted_command: std::str::SplitWhitespace<'a>) -> Option<Command>{
        if let Some(variable_to_get) = splitted_command.next() {
            return Command::new_get(variable_to_get.to_owned(), self.source_addr).into();
        }
        None
    }
    
    fn parse_set_command<'a>(&self, mut splitted_command: std::str::SplitWhitespace<'a>) -> Option<Command>{
        let variable_to_set = splitted_command.next();
        let variable_value = splitted_command.next();

        if variable_to_set.is_some() && variable_value.is_some() {
            return Command::new_set(variable_to_set.unwrap().to_owned(),
                                    variable_value.unwrap().to_owned(), self.source_addr)
                                    .into();
        }
        None
    }
}


async fn accept_loop<T>(addr: T) -> Result<()> 
    where T: ToSocketAddrs
{

    let listener = TcpListener::bind(addr).await?; 

    println!("Server started");
    let (sender, receiver) = mpsc::unbounded();
    task::spawn(handle_mem_data(receiver));
    let mut incoming = listener.incoming();
    while let Some(stream) = incoming.next().await {
        let stream = stream?;
        let peer_addr = stream.peer_addr()?;
        println!("Client connected from {}, with port {}", peer_addr.ip(), peer_addr.port());
        task::spawn(handle_client(stream, sender.clone()));
        
    }
    Ok(())
}

async fn handle_mem_data(mut receiver: Receive<Option<Command>>) -> Result<()>{

    // Do you think that create a structure to abstract all that operation and do something like
    // let data_manager = DataManager::new(receiver);
    // data_manager.start().await?
    // will be better ?


    let mut mem_data : HashMap<String, String> = HashMap::new();
    let mut peers: HashMap<SocketAddr, Sender<String>> = HashMap::new();

    while let Some(event_wrapper) = receiver.next().await {
        match event_wrapper {
            Some(event) => match event {
                Command::Get{ ref variable_name, from } => {

                    let data_got = mem_data.get(variable_name);
                    if let Some(mut peer) = peers.get(&from) {
                        if let Some(data) = data_got {
                            println!("Got variable : '{}' value is '{}'", variable_name, data);
                            let mut data = data.to_string();
                            data.push('\n');
                            peer.send(data).await?;
                        } else {
                            let send_back = format!("Unexisting variable '{}'\n", variable_name);
                            println!("{}", send_back);
                            peer.send(send_back).await?;
                        }
                    }

                },

                Command::Set{ variable_name, variable_value, from } => {

                    let old_data = mem_data.insert(variable_name.clone(), variable_value.clone());
                    if let Some(mut peer) = peers.get(&from) {
                        if let Some(data) = old_data {
                            let send_back = format!("Variable '{}' has been updated with '{}', old value was '{}'\n", variable_name, variable_value, data);
                            println!("{}", send_back );
                            peer.send(send_back).await?;
                        } else {
                            let send_back = format!("Variable '{}' has been updated with '{}'\n", variable_name, variable_value);
                            println!("{}", send_back);
                            peer.send(send_back).await?;
                        }
                    }

                },
                Command::Unknown{ command_name, from } => {
                    if let Some(mut peer) = peers.get(&from) {
                        
                        let send_back = format!("The command '{}' does not exist\n", command_name);
                        println!("{}", send_back);
                        peer.send(send_back).await?;
                        
                    }

                },
                Command::NewClient(stream) => {
                    println!("New client stream");
                    let peer_addr = stream.peer_addr().unwrap();
                    let (sender, receiver) = mpsc::unbounded();
                    peers.insert(peer_addr, sender);
                    task::spawn(handle_client_response(receiver, stream));
                },
            }
            _ => println!("Command not found"),
        }
    }

    Ok(())
}

async fn handle_client(stream: TcpStream, mut sender: Sender<Option<Command>>) -> Result<()> {
    let stream = Arc::new(stream);
    let buf_reader = BufReader::new(stream.as_ref());
    let mut lines = buf_reader.lines();
    
    sender.send(Command::NewClient(Arc::clone(&stream)).into()).await.unwrap();
    let command_parser = CommandParser::new(stream.peer_addr().unwrap());

    while let Some(line) = lines.next().await {
        let line = line?;
        // I guess that if that function below takes too long to be finished it could slow down the entire program (for real it's fast)
        // What can i do to improve and make it async to have no bottleneck ?
        let commmand = command_parser.parse_command(&line);
        sender.send(commmand).await.unwrap();
    }

    Ok(())
}

async fn handle_client_response(mut messages: Receive<String>, stream: Arc<TcpStream>) -> Result<()> {
    while let Some(message) = messages.next().await {
        stream.as_ref().write_all(message.as_bytes()).await?
    }
    Ok(())
}

fn main() {
    let result = task::block_on(accept_loop("127.0.0.1:8080"));
}
