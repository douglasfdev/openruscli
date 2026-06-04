use crate::{
    application::use_cases::process_prompt::ProcessPromptUseCase,
    domain::{
        model::{Message, Role},
        vo::SessionId,
    },
    shared::error::AppError,
};
use futures::StreamExt;
use rustyline::{error::ReadlineError, Editor, history::DefaultHistory};
use std::io::{self, Write};
use std::sync::Arc;

pub async fn run_interactive_session(use_case: Arc<ProcessPromptUseCase>) -> Result<(), AppError> {
    let mut rl = Editor::<(), DefaultHistory>::new()?;
    let session_id = SessionId::new();

    println!("Sessão iniciada: {}", session_id);
    println!("Digite '.exit' para sair.");

    loop {
        let readline = rl.readline(">> ");
        match readline {
            Ok(line) => {
                if line.trim() == ".exit" {
                    break;
                }
                if line.trim().is_empty() {
                    continue;
                }

                let _ = rl.add_history_entry(line.as_str());

                let execution_result = use_case.execute(&session_id, &line).await;
                match execution_result {
                    Ok((mut session, mut stream)) => {
                        let mut full_response = String::new();
                        print!("\nIA: ");
                        io::stdout().flush()?;

                        loop {
                            let next_item: Option<Result<String, AppError>> = stream.as_mut().next().await;
                            match next_item {
                                Some(Ok(chunk)) => {
                                    print!("{}", chunk);
                                    io::stdout().flush()?;
                                    full_response.push_str(&chunk);
                                }
                                Some(Err(e)) => {
                                    eprintln!("\nErro no stream: {}", e);
                                    break;
                                }
                                None => {
                                    break;
                                }
                            }
                        }
                        println!("\n");

                        session.messages.push(Message {
                            role: Role::Assistant,
                            content: full_response,
                            tool_calls: None,
                            tool_call_id: None,
                        });

                        if let Err(e) = use_case.save_session(&session).await {
                            eprintln!("Erro ao salvar a sessão: {}", e);
                        }
                    }
                    Err(e) => {
                        eprintln!("Erro: {}", e);
                    }
                }
            }
            Err(ReadlineError::Interrupted) => {
                println!("CTRL-C");
                break;
            }
            Err(ReadlineError::Eof) => {
                println!("CTRL-D");
                break;
            }
            Err(err) => {
                return Err(AppError::from(err));
            }
        }
    }

    Ok(())
}