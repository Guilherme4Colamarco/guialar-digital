//! Operações de proteção: **proteger** (do zero, trocar de modo, reaplicar) e **desfazer**.
//!
//! É a única interface que a linha de comando, as duas janelas e o processo elevado usam para
//! mudar a proteção. Aqui moram a decisão (o que fazer a partir do registro e do DNS em uso),
//! a ordem dos passos e o resultado, devolvido como tipo: quem chama só escolhe o texto.
//!
//! O que é do sistema (trocar o DNS, elevar, políticas de navegador) fica atrás do [`Engine`]:
//! em produção é o [`crate::platform::System`]; nos testes, um falso que anota as chamadas.

use anyhow::{Context, Result};

use crate::config::{DnsStateManifest, FilterMode};
use crate::platform::ApplyStep;

/// O que as operações pedem ao sistema. Cada método cuida da própria elevação: quem não é
/// administrador reexecuta o programa elevado e espera terminar.
pub trait Engine {
    /// DNS configurados agora (para decidir; não altera nada).
    fn configured_dns(&self) -> Vec<std::net::IpAddr>;
    /// Liga o filtro do zero. `with_browsers`: no Windows, também o bloqueador de anúncios.
    fn apply(&self, with_browsers: bool) -> Result<()>;
    /// Reaplica o filtro do modo atual mantendo o DNS original guardado em `record`.
    fn switch(&self, record: DnsStateManifest) -> Result<()>;
    /// Devolve o DNS guardado em `record` e apaga o registro.
    fn revert(&self, record: &DnsStateManifest) -> Result<()>;
    /// Sem registro utilizável: volta o DNS ao automático da rede.
    fn revert_without_record(&self) -> Result<()>;
    /// Sem DNS para desfazer: tira só o que o programa pôs nos navegadores (Windows).
    fn revert_browsers_only(&self) -> Result<()>;
}

/// O que um «proteger» fez (ou faria: ver [`protect`], que avisa antes de agir).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protected {
    /// Já estava protegido com o DNS do modo pedido: nada foi alterado.
    AlreadyActive,
    /// Não havia registro: aplicado do zero.
    Fresh,
    /// Havia um registro de uma proteção que já não estava ligada: aplicado do zero.
    ReplacedStaleRecord,
    /// Estava protegido em outro modo: o filtro foi trocado, o DNS original foi mantido.
    SwitchedMode { from: FilterMode },
    /// O registro era do mesmo modo, mas o DNS em uso não era só o do filtro: reaplicado.
    Reapplied,
}

/// O que um «desfazer» fez.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unprotected {
    /// O DNS guardado no registro foi devolvido.
    Restored,
    /// Não havia registro nem filtro em uso: nada de DNS a desfazer.
    NothingToUndo,
    /// O filtro estava em uso sem registro: o DNS voltou ao automático da rede.
    ResetWithoutRecord,
    /// O registro estava danificado: foi apagado e o DNS voltou ao automático da rede.
    ResetCorruptRecord,
}

/// Liga a proteção no `mode`, com o sistema real.
pub fn protect(
    mode: FilterMode,
    with_browsers: bool,
    before_acting: impl FnOnce(Protected),
) -> Result<Protected> {
    protect_with(&crate::platform::System, mode, with_browsers, before_acting)
}

/// Desliga a proteção, com o sistema real.
pub fn unprotect() -> Result<Unprotected> {
    unprotect_with(&crate::platform::System)
}

/// Liga a proteção. Decide uma vez (registro + DNS em uso), chama `before_acting` com o que
/// vai fazer (para quem quer avisar antes do pedido de senha) e só então age. Idempotente:
/// um segundo «proteger» não regrava o registro com o DNS do filtro como «original».
pub fn protect_with(
    engine: &dyn Engine,
    mode: FilterMode,
    with_browsers: bool,
    before_acting: impl FnOnce(Protected),
) -> Result<Protected> {
    let _guard = crate::sysutil::guard_operation()?;
    // os backends leem o modo daqui; este é o único lugar que o grava para uma operação
    crate::config::set_mode(mode);
    let step = crate::platform::next_apply_step(|| engine.configured_dns())?;
    let outcome = match &step {
        ApplyStep::AlreadyActive => Protected::AlreadyActive,
        ApplyStep::Fresh => Protected::Fresh,
        ApplyStep::Stale => Protected::ReplacedStaleRecord,
        ApplyStep::Switch(record) if record.mode != mode => {
            Protected::SwitchedMode { from: record.mode }
        }
        ApplyStep::Switch(_) => Protected::Reapplied,
    };
    before_acting(outcome);
    match step {
        ApplyStep::AlreadyActive => {}
        ApplyStep::Switch(record) => engine.switch(record)?,
        ApplyStep::Fresh | ApplyStep::Stale => engine.apply(with_browsers)?,
    }
    Ok(outcome)
}

/// Desliga a proteção. Cobre registro normal, ausente e danificado; nunca deixa o DNS preso
/// no filtro. Um registro que não pôde ser LIDO (aberto por outro programa) pode estar
/// perfeito: não é apagado nem tratado como ausente, e nada é alterado.
pub fn unprotect_with(engine: &dyn Engine) -> Result<Unprotected> {
    let _guard = crate::sysutil::guard_operation()?;
    match DnsStateManifest::load() {
        Ok(Some(record)) => {
            engine.revert(&record)?;
            Ok(Unprotected::Restored)
        }
        Ok(None) => {
            let filter_in_use = engine
                .configured_dns()
                .iter()
                .any(crate::config::is_filter_ip);
            if filter_in_use {
                engine.revert_without_record()?;
                Ok(Unprotected::ResetWithoutRecord)
            } else {
                engine.revert_browsers_only()?;
                Ok(Unprotected::NothingToUndo)
            }
        }
        Err(e) if crate::statedir::is_state_io_error(&e) => Err(e).context(
            "Nada foi alterado; feche o programa que mantém o registro aberto e desfaça de novo",
        ),
        Err(_) => {
            if let Ok(path) = DnsStateManifest::file_path() {
                let _ = std::fs::remove_file(path);
            }
            engine.revert_without_record()?;
            Ok(Unprotected::ResetCorruptRecord)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::BackendKind;
    use crate::testenv::Sandbox;
    use std::cell::RefCell;
    use std::net::IpAddr;

    /// Sistema de mentira: anota o que foi pedido e devolve o DNS combinado.
    #[derive(Default)]
    struct Fake {
        dns: Vec<IpAddr>,
        calls: RefCell<Vec<String>>,
        fail: Option<&'static str>,
    }

    impl Fake {
        fn with_dns(ips: &[&str]) -> Self {
            Fake {
                dns: ips.iter().map(|s| s.parse().unwrap()).collect(),
                ..Fake::default()
            }
        }
        fn note(&self, what: String) -> Result<()> {
            let name = what.split(' ').next().unwrap_or("").to_string();
            self.calls.borrow_mut().push(what);
            match self.fail {
                Some(f) if f == name => anyhow::bail!("falhou: {name}"),
                _ => Ok(()),
            }
        }
        fn calls(&self) -> Vec<String> {
            self.calls.borrow().clone()
        }
    }

    impl Engine for Fake {
        fn configured_dns(&self) -> Vec<IpAddr> {
            self.dns.clone()
        }
        fn apply(&self, with_browsers: bool) -> Result<()> {
            self.note(format!("apply browsers={with_browsers}"))
        }
        fn switch(&self, record: DnsStateManifest) -> Result<()> {
            self.note(format!("switch from={}", record.mode.cli_name()))
        }
        fn revert(&self, record: &DnsStateManifest) -> Result<()> {
            self.note(format!("revert mode={}", record.mode.cli_name()))
        }
        fn revert_without_record(&self) -> Result<()> {
            self.note("revert_without_record".into())
        }
        fn revert_browsers_only(&self) -> Result<()> {
            self.note("revert_browsers_only".into())
        }
    }

    const FAMILY: [&str; 2] = ["1.1.1.3", "1.0.0.3"];

    /// Grava um registro como o processo elevado gravaria, no modo `mode`.
    fn record(mode: FilterMode) {
        crate::config::set_mode(mode);
        DnsStateManifest::preflight().unwrap();
        DnsStateManifest::new(BackendKind::NetworkManager)
            .save_atomic()
            .unwrap();
        assert!(DnsStateManifest::load().unwrap().is_some(), "registro lido");
    }

    fn protect_noting(f: &Fake, mode: FilterMode) -> (Protected, Option<Protected>) {
        let mut seen = None;
        let out = protect_with(f, mode, true, |p| seen = Some(p)).unwrap();
        crate::config::set_mode(FilterMode::Family);
        (out, seen)
    }

    #[test]
    fn sem_registro_aplica_do_zero_e_avisa_antes_de_agir() {
        let _s = Sandbox::new("prot-fresh");
        let f = Fake::with_dns(&["192.168.0.1"]);
        let (out, seen) = protect_noting(&f, FilterMode::Family);
        assert_eq!(out, Protected::Fresh);
        assert_eq!(seen, Some(Protected::Fresh), "avisou antes de agir");
        assert_eq!(f.calls(), ["apply browsers=true"]);
    }

    #[test]
    fn ja_protegido_no_mesmo_modo_nao_toca_em_nada() {
        let _s = Sandbox::new("prot-ativo");
        record(FilterMode::Family);
        let f = Fake::with_dns(&FAMILY);
        let (out, _) = protect_noting(&f, FilterMode::Family);
        assert_eq!(out, Protected::AlreadyActive);
        assert!(f.calls().is_empty(), "{:?}", f.calls());
    }

    #[test]
    fn outro_modo_troca_o_filtro_mantendo_o_registro() {
        let _s = Sandbox::new("prot-troca");
        record(FilterMode::Family);
        let f = Fake::with_dns(&FAMILY);
        let (out, _) = protect_noting(&f, FilterMode::AdsOnly);
        assert_eq!(
            out,
            Protected::SwitchedMode {
                from: FilterMode::Family
            }
        );
        assert_eq!(f.calls(), ["switch from=family"]);
    }

    #[test]
    fn mesmo_modo_com_dns_de_fora_no_meio_reaplica() {
        let _s = Sandbox::new("prot-reaplica");
        record(FilterMode::Family);
        let f = Fake::with_dns(&["1.1.1.3", "8.8.8.8"]);
        let (out, _) = protect_noting(&f, FilterMode::Family);
        assert_eq!(out, Protected::Reapplied);
        assert_eq!(f.calls(), ["switch from=family"]);
    }

    #[test]
    fn registro_de_protecao_que_ja_nao_esta_ligada_aplica_do_zero() {
        let _s = Sandbox::new("prot-velho");
        record(FilterMode::Family);
        let f = Fake::with_dns(&["192.168.0.1"]);
        let (out, _) = protect_noting(&f, FilterMode::Family);
        assert_eq!(out, Protected::ReplacedStaleRecord);
        assert_eq!(f.calls(), ["apply browsers=true"]);
    }

    #[test]
    fn falha_do_sistema_ao_proteger_vira_erro_e_nao_resultado() {
        let _s = Sandbox::new("prot-falha");
        let f = Fake {
            fail: Some("apply"),
            ..Fake::with_dns(&["192.168.0.1"])
        };
        let e = protect_with(&f, FilterMode::Family, false, |_| {}).unwrap_err();
        assert!(format!("{e:#}").contains("falhou: apply"));
        assert_eq!(f.calls(), ["apply browsers=false"]);
    }

    #[test]
    fn desfazer_com_registro_devolve_o_dns_guardado() {
        let _s = Sandbox::new("unprot-registro");
        record(FilterMode::AdsOnly);
        crate::config::set_mode(FilterMode::Family);
        let f = Fake::with_dns(&["94.140.14.14"]);
        assert_eq!(unprotect_with(&f).unwrap(), Unprotected::Restored);
        assert_eq!(f.calls(), ["revert mode=ads"]);
    }

    #[test]
    fn desfazer_sem_registro_e_sem_filtro_so_olha_os_navegadores() {
        let _s = Sandbox::new("unprot-nada");
        let f = Fake::with_dns(&["192.168.0.1"]);
        assert_eq!(unprotect_with(&f).unwrap(), Unprotected::NothingToUndo);
        assert_eq!(f.calls(), ["revert_browsers_only"]);
    }

    #[test]
    fn desfazer_sem_registro_com_filtro_em_uso_volta_ao_automatico() {
        let _s = Sandbox::new("unprot-orfao");
        let f = Fake::with_dns(&["1.1.1.3"]);
        assert_eq!(unprotect_with(&f).unwrap(), Unprotected::ResetWithoutRecord);
        assert_eq!(f.calls(), ["revert_without_record"]);
    }

    #[test]
    fn desfazer_com_registro_danificado_apaga_e_volta_ao_automatico() {
        let s = Sandbox::new("unprot-danificado");
        record(FilterMode::Family);
        std::fs::write(s.manifest_path(), b"{ isto nao e um registro").unwrap();
        assert!(
            DnsStateManifest::load().is_err(),
            "o registro está ilegível"
        );
        let f = Fake::with_dns(&FAMILY);
        assert_eq!(unprotect_with(&f).unwrap(), Unprotected::ResetCorruptRecord);
        assert_eq!(f.calls(), ["revert_without_record"]);
        assert!(!s.manifest_path().exists(), "o registro danificado saiu");
    }

    #[test]
    fn falha_ao_desfazer_nao_e_relatada_como_sucesso() {
        let _s = Sandbox::new("unprot-falha");
        record(FilterMode::Family);
        let f = Fake {
            fail: Some("revert"),
            ..Fake::with_dns(&FAMILY)
        };
        assert!(unprotect_with(&f).is_err());
        assert!(
            DnsStateManifest::load().unwrap().is_some(),
            "o registro continua lá para tentar de novo"
        );
    }
}
