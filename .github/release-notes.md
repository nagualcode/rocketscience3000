# rocketscience3000 {VERSION} — Jardim Sonoro (macOS, Linux e Windows)

O bundle Rocket Science da Audio Ease (2001–2013) reconstruído a partir dos binários originais: **Roger3K** (banco de vogais de três vozes, com balbucio sincronizado ao andamento), **Follo3K** (banda ressonante que segue a energia do áudio — auto-wah, grave pulsante) e **Orbit3K** (simulador de trajetória: uma fonte voando numa sala virtual, com reflexões e efeito Doppler). Os três motores reproduzem o código de máquina original **bit a bit** (55, 41 e 39 cenários idênticos), com uma interface nova em dois temas (Modern e Classic).

### 🖥 Formatos
- **macOS (Apple Silicon)**: AU v2, VST3, CLAP, apps standalone e as CLIs `roger-render`, `follo-render`, `orbit-render`.
- **Linux (x86_64)**: CLAP, VST3, **LV2**, apps standalone e as CLIs.
- **Windows (x64)**: CLAP, VST3, apps standalone e as CLIs.

### 📦 Instalação

**macOS (Apple Silicon / arm64)**
- **`roger-{VERSION}-macos.pkg`**, **`follo-{VERSION}-macos.pkg`**, **`orbit-{VERSION}-macos.pkg`**: um instalador por plug-in.
- **`rocketscience3000-{VERSION}-macos-arm64.zip`**: os três para cópia manual (AU, VST3, CLAP, Standalone, CLI).

Macs Intel compilam a partir do código-fonte. Os binários têm assinatura ad-hoc e não são notarizados; se o macOS bloquear, use `xattr -dr com.apple.quarantine <arquivo>`.

**Linux (x86_64)**
- **`roger-…`, `follo-…`, `orbit-{VERSION}-linux-x86_64.tar.gz`**: descompacte e rode `./install.sh`.
- **`rocketscience3000-{VERSION}-linux-x86_64.zip`**: cópia manual para `~/.clap`, `~/.vst3`, `~/.lv2`.

Precisa de ALSA ou JACK e de X11/OpenGL em tempo de execução.

**Windows (x64)**
- **`roger-…`, `follo-…`, `orbit-{VERSION}-windows.exe`**: instaladores.
- **`rocketscience3000-{VERSION}-windows-x64.zip`**: cópia manual para as pastas CLAP/VST3 do usuário.

Os instaladores não são assinados, então o SmartScreen pede "Mais informações → Executar assim mesmo" na primeira vez.

Guia de uso completo no [README](https://github.com/nagualcode/rocketscience3000#readme).
