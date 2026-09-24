//! O terminal volta ao normal em qualquer saída: normal, pânico e sinal (RF-516, RNF-10).
//!
//! Só dá para medir de fora. O processo que entra na sessão é um filho — este mesmo binário de
//! teste, rodando só [`cenario_do_processo_filho`] — com um pseudo-terminal no lugar do
//! terminal. O pai confere duas coisas depois de o filho terminar:
//!
//! * o **modo** do terminal, que é estado do dispositivo e sobrevive ao processo: modo canônico
//!   e eco ligados de novo;
//! * os **bytes** que o filho escreveu: a saída da tela alternativa, exatamente uma vez.
//!
//! O CI não tem terminal, e é por isso que o pseudo-terminal existe: sem ele o modo raw nem
//! liga. No Windows não há pseudo-terminal nem os sinais do RF-516, e o arquivo não compila lá.

#![cfg(unix)]

use std::fs::File;
use std::io::{self, Read, Write};
use std::os::unix::process::ExitStatusExt;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use rustix::pty::{OpenptFlags, grantpt, openpt, ptsname, unlockpt};
use rustix::termios::{LocalModes, tcgetattr};
use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};

use xmcli::ui::term::Session;

/// Entra na tela alternativa (DECSET 1049) e esconde o cursor (DECTCEM).
const ENTER: &[u8] = b"\x1b[?1049h\x1b[?25l";
/// Mostra o cursor e sai da tela alternativa.
const LEAVE: &[u8] = b"\x1b[?25h\x1b[?1049l";

/// Variável que transforma este binário de teste no processo filho, e diz o que ele faz.
const SCENARIO: &str = "XMCLI_TESTE_TERMINAL";
/// O filho termina entrando em pânico.
const PANIC: &str = "panico";
/// O filho termina normalmente — ou é morto por sinal antes, se o pai mandar.
const EXIT: &str = "saida";

/// O filho escreve isto quando a sessão já está aberta.
const READY: &[u8] = b"<<pronto>>";
/// O filho escreve isto antes de sair normalmente, dentro da sessão.
const FRAME: &[u8] = b"<<quadro>>";
/// Mensagem do pânico do filho.
const PANIC_MESSAGE: &str = "quadro com defeito";

/// Quanto o pai espera por qualquer passo do filho antes de desistir.
///
/// Folgado de propósito: o que se mede aqui é ordem, não tempo. Estourar isto é filho travado.
const STEP_TIMEOUT: Duration = Duration::from_secs(10);

/// Intervalo entre as perguntas "o filho já terminou?".
const POLL_PERIOD: Duration = Duration::from_millis(5);

/// Quanto esperar pelos últimos bytes depois de o filho terminar.
///
/// Ler o mestre depois do fim do escravo dá fim de arquivo no Linux, mas nem todo sistema
/// promete isso; o prazo impede que o teste dependa da promessa.
const DRAIN_TIMEOUT: Duration = Duration::from_millis(500);

/// O papel do filho: abre a sessão, avisa, espera um byte do pai e termina como o cenário pede.
///
/// Sem a variável de ambiente, não faz nada — é assim que ele passa quando a suíte roda.
#[test]
fn cenario_do_processo_filho() {
    let Ok(scenario) = std::env::var(SCENARIO) else {
        return;
    };

    let mut session = Session::enter().expect("o pai deu um terminal ao filho");
    session.out().write_all(READY).expect("o pai lê o terminal");
    session.out().flush().expect("o pai lê o terminal");

    // Em modo raw o byte chega sem `Enter`. Morto por sinal, o filho nunca passa daqui.
    let mut go = [0_u8];
    io::stdin()
        .read_exact(&mut go)
        .expect("o pai escreve no terminal");

    if scenario == PANIC {
        panic!("{PANIC_MESSAGE}");
    }
    session.out().write_all(FRAME).expect("o pai lê o terminal");
    session.out().flush().expect("o pai lê o terminal");
}

/// O par mestre/escravo de um pseudo-terminal.
struct Pty {
    master: File,
    slave: File,
}

impl Pty {
    fn open() -> Self {
        let master = openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY).expect("há pseudo-terminal");
        grantpt(&master).expect("o escravo fica acessível");
        unlockpt(&master).expect("o escravo fica acessível");
        let name = ptsname(&master, Vec::new()).expect("o escravo tem nome");
        let slave = File::options()
            .read(true)
            .write(true)
            .open(name.to_str().expect("o nome do escravo é UTF-8"))
            .expect("o escravo abre");
        Self {
            master: File::from(master),
            slave,
        }
    }

    /// Modo canônico e eco — o que o modo raw desliga e a restauração tem de religar.
    fn is_cooked(&self) -> bool {
        tcgetattr(&self.slave)
            .expect("o escravo é um terminal")
            .local_modes
            .contains(LocalModes::ICANON | LocalModes::ECHO)
    }
}

/// Como o filho terminou, e o que ficou no terminal.
struct Outcome {
    status: ExitStatus,
    output: Vec<u8>,
    cooked: bool,
}

/// Roda o filho no cenário dado. Com `signal`, o mata com ele em vez de deixá-lo seguir.
fn run(scenario: &str, signal: Option<i32>) -> Outcome {
    let pty = Pty::open();
    let child = spawn_child(scenario, &pty.slave);
    let bytes = read_in_background(&pty.master);

    let mut output = Vec::new();
    collect_until(&bytes, &mut output, READY, &child);
    assert!(
        !pty.is_cooked(),
        "a sessão está aberta e o terminal não entrou em modo raw — a restauração não teria o \
         que desfazer"
    );

    let mut child = child;
    match signal {
        Some(signal) => {
            let status = Command::new("kill")
                .arg(format!("-{signal}"))
                .arg(child.id().to_string())
                .status()
                .expect("há `kill`");
            assert!(status.success(), "o sinal {signal} foi entregue");
        }
        None => {
            let mut master = pty.master.try_clone().expect("o mestre duplica");
            master.write_all(b"g").expect("o filho lê o terminal");
        }
    }

    let status = wait_with_timeout(&mut child);
    let cooked = pty.is_cooked();

    // Sem nenhum escravo aberto, o mestre chega ao fim e a linha leitora termina.
    drop(pty);
    drain(&bytes, &mut output);

    Outcome {
        status,
        output,
        cooked,
    }
}

fn spawn_child(scenario: &str, slave: &File) -> Child {
    let stdio = || Stdio::from(slave.try_clone().expect("o escravo duplica"));
    Command::new(std::env::current_exe().expect("o binário de teste existe"))
        .args(["--exact", "cenario_do_processo_filho", "--nocapture"])
        .env(SCENARIO, scenario)
        .stdin(stdio())
        .stdout(stdio())
        .stderr(stdio())
        .spawn()
        .expect("o filho começa")
}

/// Lê o mestre numa linha à parte, para o filho nunca travar com o terminal cheio.
fn read_in_background(master: &File) -> Receiver<Vec<u8>> {
    let mut master = master.try_clone().expect("o mestre duplica");
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buffer = [0_u8; 4096];
        while let Ok(read) = master.read(&mut buffer) {
            if read == 0 || sender.send(buffer[..read].to_vec()).is_err() {
                break;
            }
        }
    });
    receiver
}

fn collect_until(bytes: &Receiver<Vec<u8>>, output: &mut Vec<u8>, marker: &[u8], child: &Child) {
    let deadline = Instant::now() + STEP_TIMEOUT;
    while find(output, marker).is_none() {
        let left = deadline.saturating_duration_since(Instant::now());
        match bytes.recv_timeout(left) {
            Ok(chunk) => output.extend(chunk),
            Err(_) => panic!(
                "o filho {} não escreveu {:?}; saída até aqui: {:?}",
                child.id(),
                String::from_utf8_lossy(marker),
                String::from_utf8_lossy(output)
            ),
        }
    }
}

fn drain(bytes: &Receiver<Vec<u8>>, output: &mut Vec<u8>) {
    loop {
        match bytes.recv_timeout(DRAIN_TIMEOUT) {
            Ok(chunk) => output.extend(chunk),
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => return,
        }
    }
}

fn wait_with_timeout(child: &mut Child) -> ExitStatus {
    let deadline = Instant::now() + STEP_TIMEOUT;
    loop {
        if let Some(status) = child.try_wait().expect("o filho existe") {
            return status;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("o filho {} não terminou", child.id());
        }
        std::thread::sleep(POLL_PERIOD);
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn count(haystack: &[u8], needle: &[u8]) -> usize {
    haystack
        .windows(needle.len())
        .filter(|window| *window == needle)
        .count()
}

/// O terminal voltou, e a tela alternativa foi deixada uma vez só.
fn assert_restored(outcome: &Outcome) {
    let text = String::from_utf8_lossy(&outcome.output);
    assert!(
        outcome.cooked,
        "o terminal ficou em modo raw; saída: {text:?}"
    );
    assert_eq!(count(&outcome.output, ENTER), 1, "saída: {text:?}");
    assert_eq!(
        count(&outcome.output, LEAVE),
        1,
        "a tela alternativa tem de ser deixada exatamente uma vez; saída: {text:?}"
    );
}

#[test]
fn a_saida_normal_restaura_o_terminal() {
    let outcome = run(EXIT, None);

    assert!(
        outcome.status.success(),
        "o filho saiu com {}",
        outcome.status
    );
    assert_restored(&outcome);
    let frame = find(&outcome.output, FRAME).expect("o filho desenhou");
    let leave = find(&outcome.output, LEAVE).expect("a tela foi deixada");
    assert!(frame < leave, "o desenho fica dentro da tela alternativa");
}

#[test]
fn o_panico_restaura_o_terminal_antes_da_mensagem() {
    let outcome = run(PANIC, None);

    assert!(
        !outcome.status.success(),
        "o pânico chegou ao código de saída"
    );
    assert_restored(&outcome);
    // A ordem é o que prova o hook: o `Drop` também restauraria, mas só depois de a mensagem
    // ter sido escrita na tela alternativa — onde ela some. E com `panic = "abort"` não há
    // `Drop` nenhum.
    let leave = find(&outcome.output, LEAVE).expect("a tela foi deixada");
    let message = find(&outcome.output, PANIC_MESSAGE.as_bytes()).expect("a mensagem apareceu");
    assert!(
        leave < message,
        "a mensagem do pânico foi escrita antes de sair da tela alternativa"
    );
}

fn assert_signal_restores(signal: i32) {
    let outcome = run(EXIT, Some(signal));

    assert_eq!(
        outcome.status.signal(),
        Some(signal),
        "o filho termina pelo próprio sinal, não com código de saída: {}",
        outcome.status
    );
    assert_restored(&outcome);
    assert_eq!(
        find(&outcome.output, FRAME),
        None,
        "o sinal chegou antes do quadro"
    );
}

#[test]
fn sigterm_restaura_o_terminal() {
    assert_signal_restores(SIGTERM);
}

#[test]
fn sigint_restaura_o_terminal() {
    assert_signal_restores(SIGINT);
}

#[test]
fn sighup_restaura_o_terminal() {
    assert_signal_restores(SIGHUP);
}
