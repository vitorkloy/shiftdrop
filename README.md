# ShiftDrop

Versão Windows do [Tangerine](https://tangerineformac.com/) (app de Mac). **Comece a arrastar um arquivo no Explorer e toque uma tecla**: aparece uma roda em volta do cursor. Solte o arquivo na opção desejada. Tudo roda **localmente** e a saída vai para a mesma pasta, sem sobrescrever (`nome (1).ext`).

Feito com Tauri v2 (Rust + HTML/JS simples). Fica na bandeja do sistema.

![Comparação com o Tangerine](docs/compare-todas.png)

## As três rodas

| Tecla (durante o arraste) | Roda | Para quê |
|---|---|---|
| **Shift** | Convert formats | converter para outro formato |
| **Ctrl** | File tools | comprimir, metadados, cortar, juntar… |
| **Alt** | File actions | copiar, mover, copiar caminho, terminal (vale para pastas também) |

A roda fica aberta até você soltar. Dá para trocar de roda no meio do arraste tocando outra tecla. Soltar no centro cancela. Para trocar as teclas, use `SHIFTDROP_KEYS=convert,tools,actions` (ex. `ctrl,shift,alt`).

Forma e layout seguem o Tangerine; o acabamento é Windows 11: tema claro/escuro do sistema, cor de acento do Windows no setor ativo (fallback laranja `#ff5200`), Segoe UI Variable, ícones Segoe Fluent Icons. Sem acrylic do DWM (ele ignora o recorte circular e pintava um quadrado atrás da roda); rótulos em CAPS, pílula branca no centro com a opção sob o cursor e entrada "bloom" de 200 ms. O número de setores varia por tipo: 2 viram semicírculos e 1 vira um anel inteiro. Sem botões magnéticos e sem cursor customizado.

Referências usadas: `docs/ref/` (prints do site, do app e frames do reel). Comparações lado a lado: `docs/compare-*.png`.

## O que cada roda oferece

| Tipo | Convert (Shift) | File tools (Ctrl) |
|---|---|---|
| Imagem | PNG JPG WEBP HEIC TIFF AVIF BMP PDF | COMPRESS METADATA ~~EDIT ANNOTATE ADD BG CROP REDACT~~ |
| Vídeo | MP4 MOV MKV WEBM AVI WMV GIF MP3 | COMPRESS METADATA MUTE SPEED (2×) SNAPSHOT SPLIT (metades) JOIN ~~TRIM CROP REDACT~~ |
| Áudio | MP3 M4A WAV FLAC OGG OPUS AIFF WMA | COMPRESS METADATA NORMALIZE VISUALIZE CHANNELS ~~TRIM BLEEP~~ |
| PDF | DOCX TXT (LibreOffice), JPG PNG (ImageMagick + Ghostscript), ~~PPTX XLSX EPUB MD~~ | ~~COMPRESS METADATA SPLIT ORGANIZE~~ |
| Documentos | PDF TXT HTML EPUB PNG JPG DOCX (LibreOffice), MD (pandoc) | — |
| Slides | PDF JPG PNG (LibreOffice) | — |
| Planilhas | XLSX ODS CSV TSV PDF (LibreOffice) | — |
| EPUB | DOCX TXT MD HTML (pandoc) | — |
| Legendas | SRT VTT TXT (nativo) | — |
| Compactados | ZIP TAR GZIP (tar.exe do Windows) | EXTRACT |
| **File actions (Alt)** | COPY, MOVE, COPY PATH, TERMINAL · ~~SHARE FLOAT~~ | |

Riscado = aparece desabilitado com o motivo (etapa 2). **PAGES** fica sempre desabilitado: é formato da Apple. Opções cuja ferramenta não está instalada aparecem desabilitadas, e a pílula do centro mostra o motivo (ex. "Requer LibreOffice").

Ferramentas externas são procuradas no `PATH`, nas pastas padrão ou em `SHIFTDROP_FFMPEG`, `SHIFTDROP_MAGICK`, `SHIFTDROP_SOFFICE`, `SHIFTDROP_PANDOC`:

```powershell
winget install Gyan.FFmpeg
winget install ImageMagick.ImageMagick
winget install TheDocumentFoundation.LibreOffice   # documentos, slides, planilhas
winget install JohnMacFarlane.Pandoc               # MD e EPUB
winget install ArtifexSoftware.GhostScript         # PDF → imagem
```

## Linha de comando

```powershell
shiftdrop.exe tools                          # ferramentas detectadas
shiftdrop.exe wheels foto.jpg                # as 3 rodas para o arquivo (JSON)
shiftdrop.exe convert conv:webp foto.jpg     # converte (ids conv:/tool:/act:)
shiftdrop.exe convert tool:videoSnapshots clipe.mp4
shiftdrop.exe preview foto.jpg --mode tools --hover 0   # mostra a roda (capturas/QA)
shiftdrop.exe selftest-drag foto.jpg --drop  # simula um drag OLE no próprio processo
```

## Desenvolvimento

Requisitos: Rust (MSVC), Node 18+, WebView2.

```powershell
npm install
npm run dev        # roda o app
npm test           # testes de conversão de ponta a ponta
npm run build      # instalador NSIS em src-tauri/target/release/bundle/nsis
```

## Como funciona

Uma thread consulta o botão esquerdo e Shift/Ctrl/Alt (`GetAsyncKeyState`) a cada 15 ms. Se o arraste começou no Explorer e uma das teclas foi pressionada, a roda aparece no cursor. Tocar outra tecla troca a roda (evento `wheel-mode`). Sem hook de baixo nível e sem DLL injetada.

O drop é recebido por uma janela nativa própria (`ShiftDropCatcher`, circular, por cima da roda) com um `IDropTarget` que lê o caminho (`CF_HDROP`) já no **DragEnter**, então os setores certos aparecem antes de soltar. Ao abrir, a roda sempre desenha setores genéricos e troca pelos do tipo assim que o arquivo é identificado.

Log em `%LOCALAPPDATA%\ShiftDrop\log.txt` (gesto, tecla, roda, DragEnter/Drop com caminhos, console do JS e resultado).
