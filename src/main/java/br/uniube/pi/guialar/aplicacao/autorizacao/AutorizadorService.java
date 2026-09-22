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
            if (!verificarPrivilegios()) {
                System.err.println("\n❌ Este programa precisa ser executado com privilégios administrativos.");
                System.err.println("   Execute novamente com: sudo java -jar guialar-digital.jar");
                System.err.println("   Ou: sudo ./mvnw spring-boot:run");
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
        
        Scanner scanner = new Scanner(System.in);
        String resposta = scanner.nextLine().trim().toLowerCase();
        
        return resposta.equals("s") || resposta.equals("sim");
    }

    /**
     * Verifica se o programa está sendo executado com privilégios administrativos.
     */
    private boolean verificarPrivilegios() {
        String usuario = System.getProperty("user.name");
        
        if ("root".equals(usuario)) {
            return true;
        }

        try {
            Process process = new ProcessBuilder("id", "-u")
                .redirectErrorStream(true)
                .start();

            Scanner scanner = new Scanner(process.getInputStream());
            if (scanner.hasNextInt()) {
                int uid = scanner.nextInt();
                return uid == 0;
            }
        } catch (Exception e) {
            // Ignora erro
        }

        return false;
    }
}
