use crate::{
    application::use_cases::process_prompt::ProcessPromptUseCase,
    domain::{
        model::{Message, Role, Session},
        vo::SessionId,
    },
    shared::error::AppError,
};
use futures::StreamExt;
use rustyline::{error::ReadlineError, Editor, history::DefaultHistory};
use std::{fs, io::{self, Write}, path::{Path, PathBuf}, sync::Arc};

fn find_existing_session_id(session_dir: &Path) -> Option<SessionId> {
    let mut candidates = match fs::read_dir(session_dir) {
        Ok(entries) => entries
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry
                    .path()
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| ext.eq_ignore_ascii_case("json"))
                    .unwrap_or(false)
            })
            .filter_map(|entry| {
                let metadata = entry.metadata().ok()?;
                let modified = metadata.modified().ok()?;
                let stem = entry.path().file_stem()?.to_str()?.to_string();
                Some((modified, stem))
            })
            .collect::<Vec<_>>(),
        Err(_) => Vec::new(),
    };

    candidates.sort_by_key(|(modified, _)| *modified);
    candidates.pop().map(|(_, stem)| SessionId::from_str(&stem))
}

pub async fn run_interactive_session(
    use_case: Arc<ProcessPromptUseCase>,
    session_dir: PathBuf,
) -> Result<(), AppError> {
    let mut rl = Editor::<(), DefaultHistory>::new()?;
    let session_id = find_existing_session_id(&session_dir).unwrap_or_else(SessionId::new);
    let session_path = session_dir.join(format!("{}.json", session_id));
    let session_status = if session_path.exists() {
        "retomada"
    } else {
        "iniciada"
    };

    println!("Sessão {}: {}", session_status, session_id);
    println!("Digite '.exit' para sair.");

    if !session_path.exists() {
        let session = Session::new_with_id(session_id.clone());
        use_case.save_session(&session).await?;
    }

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