package br.uniube.pi.guialar.aplicacao.autorizacao;

import br.uniube.pi.guialar.dominio.autorizacao.PlanoAcao;

import java.util.Scanner;

/**
 * Serviço para autorização de ações que modificam o sistema.
 * 
 * Fluxo de segurança obrigatório:
 * 1. Exibir plano de ações (o que será feito)
 * 2. Solicitar confirmação do usuário
 * 3. Se necessário, elevar privilégios via polkit/pkexec
 * 4. Só então executar as ações
 */
public class AutorizadorService {

    /**
     * Solicita autorização do usuário para executar o plano de ações.
     *
     * @param plano Plano de ações a ser autorizado
     * @param possuiPrivilegios se o processo já roda com permissão de administrador
     *                          (verificada por {@code PlataformaService.isAdministrador()},
     *                          de forma multiplataforma)
     * @return true se autorizado, false caso contrário
     */
    public boolean autorizar(PlanoAcao plano, boolean possuiPrivilegios) {
        if (!plano.temAcoes()) {
            return false;
        }

        plano.exibir();

        if (!confirmarUsuario()) {
            System.out.println("\n❌ Operação cancelada pelo usuário.");
            return false;
        }

        if (plano.isRequerPrivilegios() && !possuiPrivilegios) {
            System.err.println("\n❌ Este programa precisa ser executado com privilégios administrativos.");
            System.err.println("   Linux:   sudo java -jar guialar-digital.jar --cli");
            System.err.println("   Windows: execute o terminal como Administrador e rode novamente.");
            return false;
        }

        plano.setAutorizado(true);
        System.out.println("\n✓ Autorização concedida. Executando ações...\n");
        return true;
    }

    /**
     * Solicita confirmação explícita do usuário.
     */
    private boolean confirmarUsuario() {
        System.out.print("Deseja continuar? (s/N): ");
        
        Scanner scanner = new Scanner(System.in);
        String resposta = scanner.nextLine().trim().toLowerCase();
        
        return resposta.equals("s") || resposta.equals("sim");
    }
}
