use adventuresim_dialogue::{CATALOG_DIGEST, catalog, find_conversation, source_map, validate};

fn main() {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        None | Some("check") => match validate(catalog()) {
            Ok(()) => println!(
                "dialogue catalog {CATALOG_DIGEST}: {} document(s), {} authored scalar(s)",
                catalog().len(),
                source_map().len()
            ),
            Err(errors) => {
                for error in errors {
                    eprintln!("{error:?}");
                }
                std::process::exit(1);
            }
        },
        Some("explain") => {
            let id = args.next().unwrap_or_else(|| {
                eprintln!("usage: dialogue-check explain <conversation-id>");
                std::process::exit(2)
            });
            let Some(conversation) = find_conversation(&id) else {
                eprintln!("unknown conversation: {id}");
                std::process::exit(1)
            };
            println!("conversation {}", conversation.id);
            for topic in &conversation.topics {
                println!("  topic {} ({})", topic.id, topic.label);
                for response in &topic.responses {
                    println!("    priority {:>4}  {}", response.priority, response.id);
                }
            }
        }
        Some(command) => {
            eprintln!("unknown command: {command}; expected check or explain");
            std::process::exit(2);
        }
    }
}
