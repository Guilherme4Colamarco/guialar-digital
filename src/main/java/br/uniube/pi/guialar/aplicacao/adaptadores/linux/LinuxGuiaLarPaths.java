package br.uniube.pi.guialar.aplicacao.adaptadores.linux;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.attribute.PosixFilePermission;
import java.util.EnumSet;
import java.util.Set;

/**
 * Caminhos em ~/.guialar do usuário real (nunca /root quando invocado com sudo).
 */
public final class LinuxGuiaLarPaths {

    private LinuxGuiaLarPaths() {
    }

    public static Path diretorioDados() throws IOException {
        Path home = resolverHomeUsuarioReal();
        Path dir = home.resolve(".guialar");
        Files.createDirectories(dir);
        restringirPermissoes(dir);
        return dir;
    }

    public static Path manifestoDns() throws IOException {
        return diretorioDados().resolve("dns-manifest.properties");
    }

    /**
     * @throws IllegalStateException se o processo é root sem usuário real identificável
     */
    public static Path resolverHomeUsuarioReal() {
        String sudoUser = System.getenv("SUDO_USER");
        if (sudoUser != null && !sudoUser.isBlank() && !"root".equals(sudoUser)) {
            return Path.of("/home", sudoUser);
        }
        String pkexecUid = System.getenv("PKEXEC_UID");
        if (pkexecUid != null && !pkexecUid.isBlank()) {
            String nome = lookupNomeUsuarioPorUid(pkexecUid);
            if (nome != null) {
                Path home = Path.of("/home", nome);
                if (Files.isDirectory(home)) {
                    return home;
                }
            }
        }
        String user = System.getProperty("user.name");
        if ("root".equals(user)) {
            throw new IllegalStateException(
                "Não grave configuração como root. Abra o GuiaLar como usuário normal "
                    + "(sem sudo java). O manifesto deve ficar em ~/.guialar/ do responsável.");
        }
        return Path.of(System.getProperty("user.home"));
    }

    private static String lookupNomeUsuarioPorUid(String uid) {
        try {
            Process p = new ProcessBuilder("getent", "passwd", uid)
                .redirectErrorStream(true)
                .start();
            String linha;
            try (var reader = new java.io.BufferedReader(
                    new java.io.InputStreamReader(p.getInputStream()))) {
                linha = reader.readLine();
            }
            p.waitFor();
            if (linha == null || linha.isBlank()) {
                return null;
            }
            return linha.split(":")[0];
        } catch (Exception e) {
            return null;
        }
    }

    private static void restringirPermissoes(Path dir) throws IOException {
        try {
            Set<PosixFilePermission> perms = EnumSet.of(
                PosixFilePermission.OWNER_READ,
                PosixFilePermission.OWNER_WRITE,
                PosixFilePermission.OWNER_EXECUTE
            );
            Files.setPosixFilePermissions(dir, perms);
        } catch (UnsupportedOperationException ignored) {
            // FS sem POSIX
        }
    }
}
