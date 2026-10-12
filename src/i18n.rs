//! Translations of the strings built in Rust (status messages, lists, errors).
//! The `.slint` texts use `@tr()` and `lang/pt/LC_MESSAGES/a9-panel.po` instead.

use std::sync::atomic::{AtomicBool, Ordering};

static PORTUGUESE: AtomicBool = AtomicBool::new(false);

pub const LANGUAGES: [(&str, &str); 2] = [("en", "English"), ("pt", "Português")];

pub fn set(code: &str) {
    PORTUGUESE.store(code == "pt", Ordering::Relaxed);
}

/// Normalizes a saved language code to one we support.
pub fn current_from(code: &str) -> &'static str {
    if code == "pt" { "pt" } else { "en" }
}

/// Windows display language, read from the registry (no extra dependency needed).
pub fn system() -> &'static str {
    use std::os::windows::process::CommandExt;
    let out = std::process::Command::new("reg")
        .args(["query", r"HKCU\Control Panel\International", "/v", "LocaleName"])
        .creation_flags(0x0800_0000)
        .output();
    match out {
        Ok(o) if String::from_utf8_lossy(&o.stdout).contains("pt-") => "pt",
        _ => "en",
    }
}

/// English -> Portuguese. A single `{}` works as a placeholder for dynamic parts.
const PT: &[(&str, &str)] = &[
    // devices and errors
    ("ATK receiver not found. Plug in the dongle and close ATK Hub in the browser.", "Receptor ATK não encontrado. Conecta o dongle e fecha o ATK Hub no navegador."),
    ("Could not open the receiver: {}", "Não consegui abrir o receptor: {}"),
    ("No response from the mouse (command {}). Move the mouse to wake it up and try again", "Sem resposta do mouse (comando {}). Mexe o mouse pra acordar e tenta de novo"),
    ("Incomplete reply", "Resposta incompleta"),
    ("Incomplete battery reply", "Resposta de bateria incompleta"),
    ("The mouse did not keep the value (read-back after writing did not match)", "O mouse não guardou o valor (a leitura depois da gravação não bateu)"),
    ("Unsupported polling rate", "Taxa de polling não suportada"),
    ("DPI out of the accepted range", "DPI fora da faixa aceita"),
    ("Invalid DPI level", "Nível de DPI inválido"),
    ("Invalid button", "Botão inválido"),
    ("Invalid macro slot", "Slot de macro inválido"),
    ("Macro has more than {} events", "Macro passa de {} eventos"),
    ("The mouse did not accept the change", "O mouse não aceitou a mudança"),
    ("Nothing to undo", "Nada pra desfazer"),
    ("Invalid history line", "Linha do histórico inválida"),
    // status
    ("Looking for the mouse...", "Procurando o mouse..."),
    ("Applying {}...", "Aplicando {}..."),
    ("Saved: {}", "Salvo: {}"),
    ("Error: {}", "Erro: {}"),
    ("Reading M{}...", "Lendo M{}..."),
    ("Could not save the setting: {}", "Não consegui salvar a preferência: {}"),
    ("Could not change start with Windows: {}", "Não consegui mudar o início com o Windows: {}"),
    ("polling {} Hz", "polling {} Hz"),
    ("{} DPI levels", "{} níveis de DPI"),
    ("DPI level {}", "nível de DPI {}"),
    ("debounce {} ms", "debounce {} ms"),
    ("straight-line correction", "correção de linha reta"),
    ("ripple control", "controle de ondulação"),
    ("sleep after {} s", "dormir em {} s"),
    ("sensor sleep", "sono do sensor"),
    ("sensor sleeps after {} s", "sensor dorme em {} s"),
    ("lift-off {} mm", "lift-off {} mm"),
    ("long distance mode", "modo longa distância"),
    ("receiver light", "luz do receptor"),
    ("config {}", "config {}"),
    ("buttons back to default", "botões no padrão"),
    ("macro M{}", "macro M{}"),
    ("undo", "desfazer"),
    ("factory reset", "padrão de fábrica"),
    ("sensor mode", "modo do sensor"),
    // tray
    ("Open panel", "Abrir painel"),
    ("Refresh now", "Atualizar agora"),
    ("Quit", "Sair"),
    ("A9 Ultimate: reading...", "A9 Ultimate: lendo..."),
    ("A9 Ultimate: {}%", "A9 Ultimate: {}%"),
    ("A9 Ultimate: {}% (charging)", "A9 Ultimate: {}% (carregando)"),
    ("A9 Ultimate: no response (move the mouse)", "A9 Ultimate: sem resposta (mexe o mouse)"),
    // lists
    ("Left button", "Botão esquerdo"),
    ("Right button", "Botão direito"),
    ("Middle button", "Botão do meio"),
    ("Back", "Voltar"),
    ("Forward", "Avançar"),
    ("DPI button", "Botão de DPI"),
    ("Disabled", "Desativado"),
    ("Left click", "Clique esquerdo"),
    ("Right click", "Clique direito"),
    ("Middle click", "Clique do meio"),
    ("DPI cycle", "Ciclo de DPI"),
    ("Scroll left", "Rolar pra esquerda"),
    ("Scroll right", "Rolar pra direita"),
    ("Wheel up", "Roda pra cima"),
    ("Wheel down", "Roda pra baixo"),
    ("Once", "Uma vez"),
    ("While held", "Enquanto segura"),
    ("Until pressed again", "Até apertar de novo"),
    ("Basic", "Básico"),
    ("Competitive", "Competitivo"),
    ("Competitive MAX", "Competitivo MAX"),
    ("Off", "Desligada"),
    ("Connection & polling", "Conexão e polling"),
    ("Battery", "Bateria"),
    ("Low battery only", "Só bateria fraca"),
    ("Space", "Espaço"),
    ("press", "aperta"),
    ("release", "solta"),
    ("Other ({})", "Outro ({})"),
];

/// Translates `s` to the current language (returns it unchanged in English or when unknown).
pub fn t(s: &str) -> String {
    translate(s, PORTUGUESE.load(Ordering::Relaxed))
}

fn translate(s: &str, portuguese: bool) -> String {
    if !portuguese {
        return s.to_string();
    }
    for (en, pt) in PT {
        if *en == s {
            return pt.to_string();
        }
        if let Some((pre, post)) = en.split_once("{}") {
            if s.len() >= pre.len() + post.len() && s.starts_with(pre) && s.ends_with(post) {
                return pt.replacen("{}", &s[pre.len()..s.len() - post.len()], 1);
            }
        }
    }
    s.to_string()
}

/// Translates a template with one `{}` and fills it in.
pub fn tf(template: &str, arg: impl std::fmt::Display) -> String {
    t(template).replacen("{}", &arg.to_string(), 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_fixed_and_templated_strings() {
        assert_eq!(translate("Left click", true), "Clique esquerdo");
        assert_eq!(translate("Saved: x", true), "Salvo: x");
        assert_eq!(
            translate("No response from the mouse (command 18). Move the mouse to wake it up and try again", true),
            "Sem resposta do mouse (comando 18). Mexe o mouse pra acordar e tenta de novo"
        );
        assert_eq!(translate("something unknown", true), "something unknown");
        assert_eq!(translate("Left click", false), "Left click");
    }
}
