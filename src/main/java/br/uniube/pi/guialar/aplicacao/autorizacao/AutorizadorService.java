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
     * @return true se autorizado, false caso contrário
     */
    public boolean autorizar(PlanoAcao plano) {
        if (!plano.temAcoes()) {
            return false;
        }

        plano.exibir();

        if (!confirmarUsuario()) {
            System.out.println("\n❌ Operação cancelada pelo usuário.");
            return false;
        }

        if (plano.isRequerPrivilegios()) {
            if (!podeElevarDns()) {
                System.err.println("\n❌ Não foi possível obter autorização para alterar o DNS.");
                System.err.println("   Instale o pkexec (pacote policykit-1) ou execute em ambiente com polkit.");
                System.err.println("   A interface gráfica pedirá a senha do administrador só na hora de aplicar o DNS.");
                return false;
            }
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
        
        try {
            Scanner scanner = new Scanner(System.in);
            if (!scanner.hasNextLine()) {
                System.out.println("\n(sem entrada interativa — operação cancelada)");
                return false;
            }
            String resposta = scanner.nextLine().trim().toLowerCase();
            return resposta.equals("s") || resposta.equals("sim");
        } catch (Exception e) {
            System.out.println("\n(entrada indisponível — operação cancelada)");
            return false;
        }
    }

    /**
     * Root já elevado ou pkexec disponível para elevar só a etapa de DNS.
     */
    private boolean podeElevarDns() {
        if ("root".equals(System.getProperty("user.name"))) {
            return true;
        }
        try {
            Process process = new ProcessBuilder("id", "-u")
                .redirectErrorStream(true)
                .start();
            Scanner scanner = new Scanner(process.getInputStream());
            if (scanner.hasNextInt() && scanner.nextInt() == 0) {
                return true;
            }
        } catch (Exception ignored) {
        }
        return comandoExiste("pkexec");
    }

    private boolean comandoExiste(String comando) {
        try {
            Process process = new ProcessBuilder("which", comando)
                .redirectErrorStream(true)
                .start();
            return process.waitFor() == 0;
        } catch (Exception e) {
            return false;
        }
    }
}
