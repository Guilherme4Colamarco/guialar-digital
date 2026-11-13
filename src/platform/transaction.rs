//! A sequência segura de `apply`, troca de modo e `revert`, escrita **uma vez** para todos os
//! backends de DNS.
//!
//! Cada backend (NetworkManager, systemd-resolved, resolvconf, `/etc/resolv.conf`, PowerShell)
//! só diz *como* ler, trocar e devolver o DNS ([`DnsBackend`]); a **ordem** que impede deixar o
//! DNS trocado sem registro para desfazer mora aqui:
//!
//! 1. confere que o estado pode ser guardado (`preflight`) e lê o estado de **todas** as
//!    conexões, sem alterar nada;
//! 2. grava o registro (o estado de *antes*) **antes** da primeira mudança; se nem isso der,
//!    nada foi alterado;
//! 3. troca o DNS; se falhar no meio, devolve o que já foi trocado e apaga o registro; se o
//!    desfazer também falhar, o registro fica e a mensagem manda usar «Desativar»;
//! 4. o `revert` apaga o registro por último, e se não der só avisa (o DNS já voltou).
//!
//! Quem chama já deve estar elevado e com a trava de operação (`OpLock`).

use anyhow::{Context, Result};

use crate::config::{BackendKind, DnsStateManifest, FilterMode};

/// Um subsistema de DNS. Contrato:
/// - `inspect` só lê (e guarda o que leu como está): se falha, nada foi alterado;
/// - `point_to` pode falhar no meio e então diz até onde chegou ([`PointError`]); quem desfaz
///   passa um registro só com esses alvos, então `restore` nunca mexe no que não foi trocado;
/// - `restore` e `reset` tentam **todos** os alvos e juntam as falhas.
pub(crate) trait DnsBackend {
    fn kind(&self) -> BackendKind;

    /// Lê o estado de agora e o guarda em `m` (`connections`, `previous_resolv_conf`).
    fn inspect(&self, m: &mut DnsStateManifest) -> Result<()>;

    /// Aponta o DNS para o filtro de `mode`, nos alvos descritos por `m`.
    fn point_to(&self, m: &DnsStateManifest, mode: FilterMode) -> Result<(), PointError>;

    /// Devolve o DNS ao que `m` guardou.
    fn restore(&self, m: &DnsStateManifest) -> Result<()>;

    /// Sem registro: tira o filtro e deixa o DNS automático.
    fn reset(&self) -> Result<()>;

    /// Depois de mudar o DNS (por exemplo, limpar o cache). Não pode falhar a operação.
    fn after_change(&self) {}
}

/// Falha de `point_to` no meio do caminho.
#[derive(Debug)]
pub(crate) struct PointError {
    pub error: anyhow::Error,
    /// Ids dos alvos que podem ter sido alterados (inclui o que falhou, que pode ter mudado pela
    /// metade). `None` = o backend não sabe dizer: o desfazer vale para todos.
    pub touched: Option<Vec<String>>,
}

impl PointError {
    pub(crate) fn partial(error: anyhow::Error, touched: Vec<String>) -> Self {
        Self {
            error,
            touched: Some(touched),
        }
    }
}

impl From<anyhow::Error> for PointError {
    fn from(error: anyhow::Error) -> Self {
        Self {
            error,
            touched: None,
        }
    }
}

/// O registro restrito aos alvos que `point_to` tocou (ou inteiro, se não se sabe quais).
fn only_touched(m: &DnsStateManifest, touched: &Option<Vec<String>>) -> DnsStateManifest {
    let mut sub = m.clone();
    if let Some(ids) = touched {
        sub.connections.retain(|c| ids.contains(&c.id));
    }
    sub
}

/// Liga a proteção: do zero, com o DNS de `mode`.
pub(crate) fn apply(b: &dyn DnsBackend, mode: FilterMode) -> Result<()> {
    // se o estado não puder ser guardado com segurança, nada do DNS é alterado (ADR-031)
    DnsStateManifest::preflight()?;
    let mut m = DnsStateManifest::new(b.kind());
    m.mode = mode;
    b.inspect(&mut m)?;
    // O registro descreve o estado de ANTES: grava-o antes de trocar qualquer DNS, para que uma
    // falha no meio (adaptador que some, Wi-Fi que cai) nunca deixe DNS trocado sem registro.
    m.save_atomic().context("Nada foi alterado no DNS")?;
    if let Err(pe) = b.point_to(&m, mode) {
        // desfaz só o que foi tocado: o que não mudou não é religado nem regravado
        let e = pe.error;
        return Err(match b.restore(&only_touched(&m, &pe.touched)) {
            Ok(()) => {
                // o desfazer funcionou: o registro não tem mais o que guardar
                let _ = DnsStateManifest::remove_manifest();
                b.after_change();
                e.context("Falha ao trocar o DNS; o que já tinha sido trocado foi desfeito")
            }
            Err(re) => e.context(format!(
                "Falha ao trocar o DNS e não consegui desfazer ({re:#}); use «Desativar» para restaurar"
            )),
        });
    }
    b.after_change();
    Ok(())
}

/// Troca o filtro com a proteção ligada, mantendo o DNS original do registro. `extras` roda
/// depois do DNS (as políticas dos navegadores, no Windows) e também para voltar ao modo
/// anterior se algo falhar.
pub(crate) fn switch(
    b: &dyn DnsBackend,
    mut m: DnsStateManifest,
    mode: FilterMode,
    mut extras: impl FnMut(FilterMode) -> Result<()>,
) -> Result<()> {
    DnsStateManifest::preflight()?;
    let anterior = m.mode;
    m.mode = mode;
    // o registro com o modo novo entra ANTES de qualquer mudança
    m.save_atomic().context("Nada foi alterado no DNS")?;
    // `touched`: o que `point_to` alterou antes de falhar (`None` = tudo, ou falhou depois dele)
    let (e, touched) = match b.point_to(&m, mode) {
        Err(pe) => (pe.error, pe.touched),
        Ok(()) => match extras(mode) {
            Ok(()) => {
                b.after_change();
                return Ok(());
            }
            Err(e) => (e, None),
        },
    };
    // Volta o DNS ao modo anterior só nos alvos tocados. O registro grava o modo em que o DNS
    // realmente ficou: se o DNS voltou, o anterior, mesmo que as políticas não tenham voltado.
    let mut anterior_m = only_touched(&m, &touched);
    anterior_m.mode = anterior;
    let dns_voltou = b.point_to(&anterior_m, anterior).map_err(|pe| pe.error);
    let registro = match &dns_voltou {
        Ok(()) => {
            b.after_change();
            m.mode = anterior;
            m.save_atomic()
        }
        Err(_) => Ok(()), // o DNS ficou no novo (ou misturado): o registro novo é o que vale
    };
    let politicas = match &dns_voltou {
        Ok(()) => extras(anterior),
        Err(_) => Ok(()),
    };
    let falhas: Vec<String> = [dns_voltou, registro, politicas]
        .into_iter()
        .filter_map(|r| r.err().map(|e| format!("{e:#}")))
        .collect();
    Err(if falhas.is_empty() {
        e.context("Falha ao trocar o filtro; voltei ao modo anterior")
    } else {
        e.context(format!(
            "Falha ao trocar o filtro e não consegui voltar ao modo anterior ({}); use «Desativar» para restaurar",
            falhas.join(" | ")
        ))
    })
}

/// Desliga a proteção. Ordem: DNS, `extras` (extensões), cache e só no fim apaga o registro.
pub(crate) fn revert(
    b: &dyn DnsBackend,
    m: &DnsStateManifest,
    extras: impl FnOnce() -> Result<()>,
) -> Result<()> {
    b.restore(m)?;
    extras()?;
    b.after_change();
    remove_manifest_or_warn();
    Ok(())
}

/// Desliga a proteção sem registro utilizável: o DNS volta ao automático.
pub(crate) fn revert_fallback(
    b: &dyn DnsBackend,
    extras: impl FnOnce() -> Result<()>,
) -> Result<()> {
    b.reset()?;
    extras()?;
    b.after_change();
    remove_manifest_or_warn();
    Ok(())
}

/// Apaga o registro; se não der, avisa com clareza em vez de falhar um desfazer já concluído
/// (outra conta pode manter o arquivo aberto, no Windows).
fn remove_manifest_or_warn() {
    if let Err(e) = DnsStateManifest::remove_manifest() {
        eprintln!(
            "⚠️ O DNS foi restaurado, mas o registro da proteção não pôde ser apagado ({e:#}). Provavelmente outro programa o mantém aberto: feche-o e rode o desfazer de novo."
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ConnectionBackup;
    use crate::testenv::Sandbox;
    use std::cell::RefCell;

    /// Backend em memória: `n` alvos, um registro do que cada um aponta, e falhas programáveis.
    struct Mem {
        /// DNS de cada alvo (`None` = o original, `Some(modo)` = apontado para o filtro).
        alvos: RefCell<Vec<Option<FilterMode>>>,
        /// `point_to` falha ao chegar neste alvo (a primeira vez, ou sempre se `sempre`).
        falha_no_alvo: Option<usize>,
        falha_sempre: bool,
        falha_restore: bool,
        falha_inspect: bool,
        falhou_uma_vez: RefCell<bool>,
        log: RefCell<Vec<String>>,
    }

    impl Mem {
        fn new(n: usize) -> Self {
            Self {
                alvos: RefCell::new(vec![None; n]),
                falha_no_alvo: None,
                falha_sempre: false,
                falha_restore: false,
                falha_inspect: false,
                falhou_uma_vez: RefCell::new(false),
                log: RefCell::new(Vec::new()),
            }
        }
        fn alvos(&self) -> Vec<Option<FilterMode>> {
            self.alvos.borrow().clone()
        }
        fn log(&self) -> Vec<String> {
            self.log.borrow().clone()
        }
    }

    impl DnsBackend for Mem {
        fn kind(&self) -> BackendKind {
            BackendKind::NetworkManager
        }
        fn inspect(&self, m: &mut DnsStateManifest) -> Result<()> {
            self.log.borrow_mut().push("inspect".into());
            if self.falha_inspect {
                anyhow::bail!("leitura falhou");
            }
            for i in 0..self.alvos.borrow().len() {
                m.connections.push(ConnectionBackup {
                    id: format!("c{i}"),
                    original_ipv4_dns: Vec::new(),
                    original_ipv6_dns: Vec::new(),
                    was_dhcp_ipv4: true,
                    was_dhcp_ipv6: true,
                });
            }
            Ok(())
        }
        fn point_to(&self, m: &DnsStateManifest, mode: FilterMode) -> Result<(), PointError> {
            self.log.borrow_mut().push(format!("point {mode:?}"));
            let mut tocados = Vec::new();
            for c in &m.connections {
                let i: usize = c.id[1..].parse().unwrap();
                tocados.push(c.id.clone());
                if self.falha_no_alvo == Some(i)
                    && (self.falha_sempre || !*self.falhou_uma_vez.borrow())
                {
                    *self.falhou_uma_vez.borrow_mut() = true;
                    return Err(PointError::partial(
                        anyhow::anyhow!("set do alvo {i} falhou"),
                        tocados,
                    ));
                }
                self.alvos.borrow_mut()[i] = Some(mode);
            }
            Ok(())
        }
        fn restore(&self, m: &DnsStateManifest) -> Result<()> {
            self.log
                .borrow_mut()
                .push(format!("restore {}", m.connections.len()));
            if self.falha_restore {
                anyhow::bail!("desfazer falhou");
            }
            for c in &m.connections {
                let i: usize = c.id[1..].parse().unwrap();
                self.alvos.borrow_mut()[i] = None;
            }
            Ok(())
        }
        fn reset(&self) -> Result<()> {
            self.log.borrow_mut().push("reset".into());
            self.alvos.borrow_mut().iter_mut().for_each(|a| *a = None);
            Ok(())
        }
        fn after_change(&self) {
            self.log.borrow_mut().push("after".into());
        }
    }

    fn manifesto() -> Option<DnsStateManifest> {
        DnsStateManifest::load().unwrap()
    }

    #[test]
    fn apply_grava_o_registro_antes_de_trocar_e_depois_limpa_o_cache() {
        let _s = Sandbox::new("tx-apply-ok");
        let b = Mem::new(2);
        apply(&b, FilterMode::Family).unwrap();
        assert_eq!(b.alvos(), vec![Some(FilterMode::Family); 2]);
        let m = manifesto().expect("registro gravado");
        assert_eq!(m.connections.len(), 2);
        assert_eq!(m.mode, FilterMode::Family);
        assert_eq!(b.log(), ["inspect", "point Family", "after"]);
    }

    #[test]
    fn apply_le_todos_antes_de_mexer_em_qualquer_um() {
        let _s = Sandbox::new("tx-apply-leitura");
        let mut b = Mem::new(2);
        b.falha_inspect = true;
        assert!(apply(&b, FilterMode::Family).is_err());
        assert_eq!(b.alvos(), vec![None; 2]);
        assert_eq!(b.log(), ["inspect"], "nada foi trocado");
        assert!(manifesto().is_none());
    }

    #[test]
    fn apply_que_falha_no_meio_desfaz_e_nao_deixa_registro() {
        let _s = Sandbox::new("tx-apply-falha");
        let mut b = Mem::new(2);
        b.falha_no_alvo = Some(1);
        let e = format!("{:#}", apply(&b, FilterMode::Family).unwrap_err());
        assert!(
            e.contains("foi desfeito") && e.contains("set do alvo 1"),
            "{e}"
        );
        assert_eq!(b.alvos(), vec![None; 2]);
        assert!(manifesto().is_none());
        // o desfazer deu certo: o cache também é limpo (Windows)
        assert_eq!(b.log().last().map(String::as_str), Some("after"));
    }

    #[test]
    fn apply_que_falha_desfaz_so_os_alvos_tocados() {
        let _s = Sandbox::new("tx-apply-so-tocados");
        let mut b = Mem::new(3);
        b.falha_no_alvo = Some(1);
        assert!(apply(&b, FilterMode::Family).is_err());
        // c0 foi trocado e c1 falhou pela metade; c2 nunca foi tocado e não é desfeito
        assert!(b.log().contains(&"restore 2".to_string()), "{:?}", b.log());
        assert_eq!(b.alvos(), vec![None; 3]);
    }

    #[test]
    fn apply_que_falha_e_nao_desfaz_deixa_o_registro_para_o_desativar() {
        let _s = Sandbox::new("tx-apply-falha-2");
        let mut b = Mem::new(2);
        b.falha_no_alvo = Some(1);
        b.falha_restore = true;
        let e = format!("{:#}", apply(&b, FilterMode::Family).unwrap_err());
        assert!(
            e.contains("não consegui desfazer") && e.contains("Desativar"),
            "{e}"
        );
        assert_eq!(manifesto().expect("registro fica").connections.len(), 2);
    }

    fn ligado(nome: &str) -> (Sandbox, Mem, DnsStateManifest) {
        let s = Sandbox::new(nome);
        let b = Mem::new(2);
        apply(&b, FilterMode::Family).unwrap();
        let m = manifesto().unwrap();
        b.log.borrow_mut().clear();
        (s, b, m)
    }

    #[test]
    fn troca_de_modo_grava_o_registro_novo_antes_e_roda_os_extras() {
        let (_s, b, m) = ligado("tx-switch-ok");
        let vistos = RefCell::new(Vec::new());
        switch(&b, m, FilterMode::AdsOnly, |modo| {
            // quando os extras rodam, o registro já tem o modo novo e o DNS já foi trocado
            vistos
                .borrow_mut()
                .push((modo, manifesto().unwrap().mode, b.alvos()[0]));
            Ok(())
        })
        .unwrap();
        assert_eq!(
            vistos.into_inner(),
            [(
                FilterMode::AdsOnly,
                FilterMode::AdsOnly,
                Some(FilterMode::AdsOnly)
            )]
        );
        assert_eq!(b.alvos(), vec![Some(FilterMode::AdsOnly); 2]);
        assert_eq!(manifesto().unwrap().mode, FilterMode::AdsOnly);
    }

    #[test]
    fn troca_de_modo_que_falha_no_meio_volta_dns_extras_e_registro() {
        let (_s, mut b, m) = ligado("tx-switch-falha");
        b.falha_no_alvo = Some(1); // só na primeira vez: o desfazer passa
        let extras = RefCell::new(Vec::new());
        let e = switch(&b, m, FilterMode::AdsOnly, |modo| {
            extras.borrow_mut().push(modo);
            Ok(())
        })
        .unwrap_err();
        assert!(
            format!("{e:#}").contains("voltei ao modo anterior"),
            "{e:#}"
        );
        assert_eq!(b.alvos(), vec![Some(FilterMode::Family); 2]);
        assert_eq!(manifesto().unwrap().mode, FilterMode::Family);
        // falhou antes dos extras: só o desfazer os roda, no modo anterior
        assert_eq!(extras.into_inner(), [FilterMode::Family]);
    }

    #[test]
    fn troca_de_modo_com_extras_que_falham_roda_os_do_modo_anterior() {
        let (_s, b, m) = ligado("tx-switch-extras");
        let extras = RefCell::new(Vec::new());
        let e = switch(&b, m, FilterMode::AdsOnly, |modo| {
            extras.borrow_mut().push(modo);
            if modo == FilterMode::AdsOnly {
                anyhow::bail!("reg.exe falhou");
            }
            Ok(())
        })
        .unwrap_err();
        let e = format!("{e:#}");
        assert!(
            e.contains("voltei ao modo anterior") && e.contains("reg.exe falhou"),
            "{e}"
        );
        assert_eq!(
            extras.into_inner(),
            [FilterMode::AdsOnly, FilterMode::Family]
        );
        assert_eq!(b.alvos(), vec![Some(FilterMode::Family); 2]);
    }

    #[test]
    fn troca_de_modo_que_nao_consegue_voltar_manda_usar_desativar() {
        let (_s, mut b, m) = ligado("tx-switch-falha-2");
        b.falha_no_alvo = Some(1);
        b.falha_sempre = true;
        let e = format!(
            "{:#}",
            switch(&b, m, FilterMode::AdsOnly, |_| Ok(())).unwrap_err()
        );
        assert!(
            e.contains("não consegui voltar ao modo anterior") && e.contains("Desativar"),
            "{e}"
        );
    }

    #[test]
    fn troca_de_modo_que_falha_no_meio_volta_so_os_alvos_tocados() {
        let _s = Sandbox::new("tx-switch-so-tocados");
        let mut b = Mem::new(3);
        apply(&b, FilterMode::Family).unwrap();
        let m = manifesto().unwrap();
        b.falha_no_alvo = Some(1);
        b.log.borrow_mut().clear();
        assert!(switch(&b, m, FilterMode::AdsOnly, |_| Ok(())).is_err());
        // c2 nunca foi tocado: continua no modo anterior, sem ser reapontado
        assert_eq!(b.alvos(), vec![Some(FilterMode::Family); 3]);
        assert_eq!(manifesto().unwrap().mode, FilterMode::Family);
        assert_eq!(
            b.log(),
            ["point AdsOnly", "point Family", "after"],
            "só um point_to de volta, e o cache limpo"
        );
    }

    #[test]
    fn troca_de_modo_com_politicas_que_nao_voltam_grava_o_modo_do_dns() {
        let (_s, b, m) = ligado("tx-switch-politicas");
        let e = switch(&b, m, FilterMode::AdsOnly, |_| {
            anyhow::bail!("reg.exe falhou")
        })
        .unwrap_err();
        let e = format!("{e:#}");
        assert!(
            e.contains("não consegui voltar ao modo anterior") && e.contains("Desativar"),
            "{e}"
        );
        // o DNS voltou ao Family: o registro diz Family, não o AdsOnly que não ficou
        assert_eq!(b.alvos(), vec![Some(FilterMode::Family); 2]);
        assert_eq!(manifesto().unwrap().mode, FilterMode::Family);
    }

    #[test]
    fn revert_restaura_roda_extras_limpa_o_cache_e_so_no_fim_apaga_o_registro() {
        let (_s, b, m) = ligado("tx-revert-ordem");
        let no_extras = RefCell::new(None);
        revert(&b, &m, || {
            *no_extras.borrow_mut() = Some(manifesto().is_some());
            Ok(())
        })
        .unwrap();
        assert_eq!(
            no_extras.into_inner(),
            Some(true),
            "o registro ainda existe"
        );
        assert_eq!(b.log(), ["restore 2", "after"]);
        assert_eq!(b.alvos(), vec![None; 2]);
        assert!(manifesto().is_none());
    }

    #[test]
    fn revert_com_extras_que_falham_mantem_o_registro() {
        let (_s, b, m) = ligado("tx-revert-extras");
        assert!(revert(&b, &m, || anyhow::bail!("reg.exe falhou")).is_err());
        assert!(manifesto().is_some(), "dá para repetir o desfazer");
    }

    #[test]
    fn revert_sem_registro_utilizavel_volta_ao_automatico() {
        let (_s, b, _m) = ligado("tx-fallback");
        revert_fallback(&b, || Ok(())).unwrap();
        assert_eq!(b.alvos(), vec![None; 2]);
        assert!(manifesto().is_none());
        assert_eq!(b.log(), ["reset", "after"]);
    }
}
