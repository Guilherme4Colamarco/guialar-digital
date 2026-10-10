package br.uniube.pi.guialar.gui;

import java.awt.Color;

/**
 * Paletas da interface. O tema claro mantém as cores originais;
 * o escuro usa tons mais claros de verde/âmbar/vermelho para manter contraste.
 */
public enum Tema {

    CLARO(
        0xF4F6F8, 0xFFFFFF, 0xE0E4E8, 0x0B5F63, 0x0B5F63, 0x0E7A80,
        0x1F2933, 0x5A636B, 0x1B7A2E, 0x8A5A00, 0xB3261A,
        0xF9FBFC, 0x1E242B, 0xE6E6E6, 0xD6EEEF),

    ESCURO(
        0x12171C, 0x1C232B, 0x2C3640, 0x0B5F63, 0x5CC9CF, 0x0E7A80,
        0xE6EAEE, 0x9AA5B1, 0x4CC38A, 0xF0B84A, 0xFF6B5E,
        0x161C22, 0x0B0F13, 0xE6E6E6, 0xD6EEEF);

    private final Color fundo, cartao, borda, cabecalho, titulo, primaria,
        texto, secundario, ok, atencao, erro, planoFundo, logFundo, logTexto, cabecalhoSub;

    Tema(int fundo, int cartao, int borda, int cabecalho, int titulo, int primaria,
         int texto, int secundario, int ok, int atencao, int erro,
         int planoFundo, int logFundo, int logTexto, int cabecalhoSub) {
        this.fundo = new Color(fundo);
        this.cartao = new Color(cartao);
        this.borda = new Color(borda);
        this.cabecalho = new Color(cabecalho);
        this.titulo = new Color(titulo);
        this.primaria = new Color(primaria);
        this.texto = new Color(texto);
        this.secundario = new Color(secundario);
        this.ok = new Color(ok);
        this.atencao = new Color(atencao);
        this.erro = new Color(erro);
        this.planoFundo = new Color(planoFundo);
        this.logFundo = new Color(logFundo);
        this.logTexto = new Color(logTexto);
        this.cabecalhoSub = new Color(cabecalhoSub);
    }

    public Color fundo() { return fundo; }
    public Color cartao() { return cartao; }
    public Color borda() { return borda; }
    public Color cabecalho() { return cabecalho; }
    public Color titulo() { return titulo; }
    public Color primaria() { return primaria; }
    public Color texto() { return texto; }
    public Color secundario() { return secundario; }
    public Color ok() { return ok; }
    public Color atencao() { return atencao; }
    public Color erro() { return erro; }
    public Color planoFundo() { return planoFundo; }
    public Color logFundo() { return logFundo; }
    public Color logTexto() { return logTexto; }
    public Color cabecalhoSub() { return cabecalhoSub; }

    public boolean isEscuro() {
        return this == ESCURO;
    }

    public Tema alternar() {
        return isEscuro() ? CLARO : ESCURO;
    }

    /** Aceita "escuro"/"dark", "claro"/"light" e "auto"; outro valor devolve null. */
    public static Tema de(String valor) {
        if (valor == null) return null;
        return switch (valor.trim().toLowerCase()) {
            case "escuro", "dark" -> ESCURO;
            case "claro", "light" -> CLARO;
            default -> null;
        };
    }
}
