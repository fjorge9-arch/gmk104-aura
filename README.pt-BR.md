# gmk104-aura

🇧🇷 Português · [🇺🇸 English](README.md)

Faz o teclado **ZUOYA GMK104** acompanhar a cor que o **ASUS Aura Sync / Armoury Crate** aplica na
placa-mãe. Se o Aura está no modo **Smart** (cor pela temperatura da CPU), o teclado fica verde,
amarelo ou vermelho junto com a placa; se está numa cor fixa, o teclado copia a cor.

Um único `.exe` de ~350 KB, sem instalador, sem administrador, sem janela: só um ícone redondo ao lado
do relógio mostrando a cor atual do teclado.

> O Armoury Crate e os outros plugins (Corsair, etc.) continuam intocados. O programa apenas **lê** o
> que o Aura já decidiu e repete no teclado.

## Instalação (usuário comum)

1. Baixe `gmk104-aura.exe` na página de [Releases](../../releases/latest).
2. Coloque-o numa pasta definitiva (por exemplo `C:\Users\você\gmk104-aura\`) e dê dois cliques.
3. Na primeira vez ele pergunta **"Iniciar junto com o Windows?"**. Responda **Sim** e pronto.

A partir daí o programa sobe sozinho a cada logon. O ícone ao lado do relógio mostra a cor atual;
clique nele para ver o menu:

| Item | O que faz |
|---|---|
| Teclado GMK104: conectado / não encontrado | estado da conexão com o teclado |
| Cor: verde — modo Smart (log do Aura) | cor atual e de onde ela veio |
| ☑ Iniciar com o Windows | liga/desliga o início automático (por usuário, sem administrador) |
| Abrir log | abre `%LOCALAPPDATA%\gmk104-aura\gmk104-aura.log` |
| Sair | encerra o programa (o teclado fica na última cor) |

**Aviso do SmartScreen.** O executável não é assinado digitalmente (assinatura custa caro). Na
primeira execução o Windows pode mostrar "O Windows protegeu o computador": clique em **Mais
informações → Executar assim mesmo**. O código é aberto, e o `.exe` das Releases é compilado
automaticamente pelo GitHub Actions a partir deste repositório.

**Desinstalar.** No menu do ícone, desmarque "Iniciar com o Windows" e clique em "Sair". Depois apague
o `.exe` e, se quiser, a pasta `%LOCALAPPDATA%\gmk104-aura` (só log e um marcador).

## Requisitos

- Windows 10/11 com **Armoury Crate** e **Aura Sync** funcionando (serviço `LightingService` ativo).
- Teclado **ZUOYA GMK104** (USB `320F:5055`; também aceita o receptor 2.4G `320F:5088`).
- Nada mais: não precisa de Python, drivers, VIA ou administrador.

## O que ele segue (e o que não)

- **Cor fixa** na placa-mãe → mesma cor no teclado.
- **Modo Smart** (térmico) → verde / amarelo / vermelho conforme a faixa que o próprio Aura escolheu,
  com as cores e limiares do seu perfil (padrão 65 °C / 85 °C). Muda por faixa, sem gradiente, igual
  ao Aura; com a CPU em cima do limiar, a placa e o teclado alternam juntos.
- **Efeitos animados** (arco-íris, respiração, música...) não são reproduzidos quadro a quadro: o
  teclado recebe a cor base do efeito.
- O **brilho** do teclado não é alterado; ajuste pelo Fn do próprio teclado.
- Se você trocar o efeito do teclado por Fn, o programa volta à cor única no próximo reforço (5 s).
- O teclado continua não aparecendo no Aura Sync, SignalRGB ou OpenRGB.

## Como funciona (para os curiosos)

```
LightingService (Aura)                          gmk104-aura                        GMK104
 LastProfile.xml ───── modo e cor base ──────┐
 LightingService.log ─ cor da faixa (Smart) ─┼─► decide a cor ──VIA Raw HID──► cor
                                             │   (20 verificações/s,
                                             │    envia só quando muda)
```

1. **Perfil**: `C:\Program Files (x86)\LightingService\LastProfile.xml` e `script\LastScript.xml` dizem
   o modo atual (key `100` = Smart), a cor base da placa e, no Smart, os limiares e as três cores.
2. **Cor da faixa**: no Smart, o LightingService escreve no log público
   `%ProgramData%\ASUS\ARMOURY CRATE Diagnosis\LightingService\LightingService.log` a linha
   `[ParseInitColor] parsed value => hue=…, saturation=…, lightness=…` toda vez que troca de faixa.
   O programa acompanha o fim desse arquivo (e os backups `.log.1`–`.log.3` logo após uma rotação).
   Por isso **não precisa de administrador**: ler a temperatura direto do serviço da ASUS exigiria.
3. **Teclado**: canal VIA Raw HID (`usage page 0xFF60`), canal 3 (`rgb_matrix`): efeito 6 (cor única)
   e cor em `hue/sat` 0..255. Só envia quando a cor muda, mais um reforço a cada 5 s, então não desgasta
   a memória do teclado. Reconecta sozinho se o teclado for desligado e religado.

Consumo medido: ~5 MB de memória e ~1 % de um núcleo por segundo de CPU, ou menos.

## Compilar do código

Precisa do [Rust](https://rustup.rs) (toolchain MSVC) e das ferramentas de build do Visual Studio
(a `hidapi` compila um pouco de C; o `rc.exe` do Windows SDK embute o ícone).

```bash
cargo build --release
```

O binário sai em `target\release\gmk104-aura.exe`. Opções de linha de comando, úteis para depurar:
`-v` (escreve o log também no terminal), `--no-tray` (sem ícone, roda no terminal), `--duration N`
(encerra após N s), `--fps N`, `--refresh S`.

Organização: `src/keyboard.rs` (VIA HID), `src/aura_profile.rs` (XML do perfil), `src/aura_log.rs`
(log do LightingService), `src/follower.rs` (laço), `src/tray.rs` (bandeja, Win32 puro),
`src/autostart.rs` (chave Run), `src/logger.rs`.

### Versão Python (referência)

Em `python/` está a implementação original, usada para descobrir o protocolo. Faz o mesmo, mais uma
fonte extra de temperatura via COM da ASUS (`aaHM.acpiHmData2`, só funciona elevado). Precisa de
`pip install -r python/requirements.txt` e roda com `python python/run.py -v`. Os scripts em
`python/tools/` (atalho/tarefa agendada) ficaram como legado: o `.exe` faz isso sozinho.

## Protocolos descobertos

### VIA Raw HID (usado)

Relatórios de 32 bytes (report id 0) na interface `0xFF60/0x61`:

| Comando | Bytes | Observação |
|---|---|---|
| set valor | `07 03 id dados…` | eco na resposta |
| get valor | `08 03 id` | valor nos bytes 3.. |
| id 1 brilho | 0..4 | o firmware limita a 4; valores maiores **apagam** o teclado |
| id 2 efeito | 6 = cor única | lista completa no JSON do VIA do teclado |
| id 3 velocidade | 0..4 | |
| id 4 cor | `hue sat` | 0..255 cada |

### Protocolo do GMK87 (gmk87-node), também aceito pelo GMK104

Interface `usage page 0xFF1C`, report id `0x04`, 64 bytes, checksum de 16 bits = soma dos bytes 3..63
nos bytes 1..2; cmd no byte 3, tamanho no 4, posição nos 5..7, dados a partir do 8. Leitura da
configuração: `0x01`, `0x03`×10, `0x02`, `0x05`×12 (dados nos bytes 4..7 da resposta). Escrita:
`0x01`, `0x06` (48 bytes), `0x02`. Layout: `[1]` efeito, `[2]` brilho, `[6..8]` RGB, `[9]` hue,
`[21]` winlock, `[35..41]` relógio BCD, `[43..44]` delay de quadro. Grava a configuração inteira
(provável flash), por isso não é usado.

## Licença

MIT. Não tem relação com a ASUS nem com a ZUOYA; nomes de produtos pertencem aos seus donos.
