package br.uniube.pi.guialar.dominio.autorizacao;

import java.util.ArrayList;
import java.util.List;

/**
 * Plano de ações que serão executadas no sistema.
 * 
 * Segue o princípio de transparência: sempre listar o que será feito
 * ANTES de pedir autorização via polkit/pkexec.
 */
public class PlanoAcao {
    private List<String> acoes = new ArrayList<>();
    private boolean requerPrivilegios = false;
    private boolean autorizado = false;

    public List<String> getAcoes() {
        return acoes;
    }

    public boolean isRequerPrivilegios() {
        return requerPrivilegios;
    }

    public void setRequerPrivilegios(boolean requerPrivilegios) {
        this.requerPrivilegios = requerPrivilegios;
    }

    public boolean isAutorizado() {
        return autorizado;
    }

    public void setAutorizado(boolean autorizado) {
        this.autorizado = autorizado;
    }

    public void adicionarAcao(String acao) {
        acoes.add(acao);
    }

    public void adicionarAcao(String acao, boolean requerRoot) {
        acoes.add(acao);
        if (requerRoot) {
            this.requerPrivilegios = true;
        }
    }

    public boolean temAcoes() {
        return !acoes.isEmpty();
    }

    public void exibir() {
        if (acoes.isEmpty()) {
            System.out.println("Nenhuma ação será executada.");
            return;
        }

        System.out.println("╔════════════════════════════════════════════════════════════════╗");
        System.out.println("║              PLANO DE AÇÕES - GuiaLar Digital                  ║");
        System.out.println("╚════════════════════════════════════════════════════════════════╝");
        System.out.println();
        System.out.println("As seguintes ações serão executadas:");
        System.out.println();

        for (int i = 0; i < acoes.size(); i++) {
            System.out.println("  " + (i + 1) + ". " + acoes.get(i));
        }

        System.out.println();

        if (requerPrivilegios) {
            System.out.println("⚠️  ATENÇÃO: Essas operações requerem privilégios administrativos.");
            System.out.println("   Você será solicitado a autorizar via polkit/pkexec.");
        }

        System.out.println();
    }
}
