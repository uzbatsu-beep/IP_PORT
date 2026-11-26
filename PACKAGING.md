# Руководство по сборке, установке и упаковке

Данный документ описывает требования к окружению, системные зависимости, инструкции по локальной сборке и процедуру упаковки бинарных релизов для операционных систем **Windows**, **Linux** и **macOS**.

**Текущий официальный релиз:** [v0.4.0](https://github.com/uzbatsu-beep/IP_PORT/releases/tag/v0.4.0) (Windows x64, Linux x64, macOS Intel/Apple Silicon); SHA-256 публикуется вместе с четырьмя платформенными архивами в `SHA256SUMS.txt`. Бинарники GitHub Actions **не равнозначны официальному релизу** и могут удаляться по сроку хранения.

---

## 1. Системные требования

### 1.1. Минимальная версия Rust (MSRV)
- **Rust:** `1.95.0` или новее (редакция `2024 edition`, обусловлено зависимостями egui 0.37).
- **Cargo:** встроенный в дистрибутив Rust.

### 1.2. Целевые платформы (Tier 1 & Tier 2)

| Платформа | Target Triple | Формат пакета | Графический бэкенд |
|---|---|---|---|
| **Windows x64** | `x86_64-pc-windows-msvc` | `.zip` | WGL / OpenGL (glow) |
| **Linux x64** | `x86_64-unknown-linux-gnu` | `.tar.gz` | Wayland / X11 / Mesa |
| **macOS Apple Silicon** | `aarch64-apple-darwin` | `.tar.gz` | Cocoa / OpenGL |
| **macOS Intel** | `x86_64-apple-darwin` | `.tar.gz` | Cocoa / OpenGL |

---

## 2. Установка зависимостей для сборки

### 2.1. Windows
Для сборки требуется компилятор MSVC (Visual Studio 2022 C++ Build Tools) и Rust:
```powershell
winget install Rustlang.Rustup
rustup default stable
```
Дополнительных системных библиотек не требуется: приложение компилируется в автономный исполняемый файл `ip_port_scanner.exe` со статическим связыванием рантайма Rust.

### 2.2. Linux (Debian, Ubuntu, Linux Mint)
Для сборки требуются заголовочные файлы X11, Wayland, xkbcommon и Mesa:
```bash
sudo apt-get update
sudo apt-get install -y \
  build-essential \
  pkg-config \
  libxkbcommon-dev \
  libwayland-dev \
  libgl1-mesa-dev \
  libx11-dev \
  libxcursor-dev \
  libxi-dev \
  libxrandr-dev
```

Для Fedora / RHEL:
```bash
sudo dnf install -y \
  gcc \
  pkgconf-pkg-config \
  libxkbcommon-devel \
  wayland-devel \
  mesa-libGL-devel \
  libX11-devel \
  libXcursor-devel \
  libXi-devel \
  libXrandr-devel
```

Для Arch Linux:
```bash
sudo pacman -S --needed base-devel libxkbcommon wayland mesa libx11 libxcursor libxi libxrandr
```

### 2.3. macOS
Требуются Command Line Tools:
```bash
xcode-select --install
```

---

## 3. Сборка из исходного кода

```bash
# Клонирование репозитория
git clone https://github.com/uzbatsu-beep/IP_PORT.git
cd IP_PORT

# Проверка форматирования и линтинг
cargo fmt --check
cargo clippy --all-targets -- -D warnings

# Запуск полного набора тестов
cargo test --all-targets
python3 scripts/check_docs.py

# Сборка оптимизированного релизного бинарника
cargo build --locked --release
```

Готовый исполняемый файл будет находиться в директории:
- **Windows:** `target\release\ip_port_scanner.exe`
- **Linux / macOS:** `target/release/ip_port_scanner`

---

## 4. Установка готового бинарного релиза

### 4.1. Windows
1. Скачайте архив `ip_port_scanner-vX.Y.Z-x86_64-pc-windows-msvc.zip` со страницы релизов.
2. **Проверьте SHA-256 скачанного ZIP** по файлу `SHA256SUMS.txt` с той же страницы:
   ```powershell
   $archive = ".\\ip_port_scanner-v0.4.0-x86_64-pc-windows-msvc.zip"
   (Get-FileHash -Path $archive -Algorithm SHA256).Hash.ToLowerInvariant()
   Get-Content ".\\SHA256SUMS.txt"
   ```
   Хэш ZIP должен точно совпадать со строкой этого архива в манифесте. Не сравнивайте SHA-256 EXE с контрольной суммой ZIP.
3. Распакуйте архив в любую удобную папку (например, `C:\Tools\ip_port_scanner`).
4. Запустите `ip_port_scanner.exe`.
5. (Опционально) Создайте ярлык на рабочем столе или добавьте путь в переменную среды `PATH`.

### 4.2. Linux
1. Скачайте архив `ip_port_scanner-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz`.
2. Распакуйте архив:
   ```bash
   tar -xzf ip_port_scanner-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz
   cd ip_port_scanner-vX.Y.Z-x86_64-unknown-linux-gnu
   ```
3. Скопируйте бинарный файл в `/usr/local/bin`:
   ```bash
   sudo install -m 755 ip_port_scanner /usr/local/bin/
   ```
4. Для интеграции в меню приложений создайте файл `~/.local/share/applications/ip_port_scanner.desktop`:
   ```ini
   [Desktop Entry]
   Name=IP / PORT Scanner
   Comment=Fast cross-platform async network scanner
   Exec=ip_port_scanner
   Terminal=false
   Type=Application
   Categories=Network;Security;
   ```

### 4.3. macOS
1. Скачайте архив для вашей архитектуры:
   - Для компьютеров Apple Silicon (M1/M2/M3/M4): `ip_port_scanner-vX.Y.Z-aarch64-apple-darwin.tar.gz`
   - Для компьютеров Intel: `ip_port_scanner-vX.Y.Z-x86_64-apple-darwin.tar.gz`
2. Распакуйте архив:
   ```bash
   tar -xzf ip_port_scanner-vX.Y.Z-aarch64-apple-darwin.tar.gz
   ```
3. При необходимости снимите атрибут карантина macOS Gatekeeper:
   ```bash
   xattr -cr ip_port_scanner
   ```
4. Переместите бинарный файл в `/usr/local/bin`:
   ```bash
   sudo cp ip_port_scanner /usr/local/bin/
   ```

---

## 5. Процесс выпуска новой версии (Release Process)

1. Обновите версию в `Cargo.toml` (`version = "X.Y.Z"`).
2. Задокументируйте изменения в [CHANGELOG.md](CHANGELOG.md) в соответствии с форматом Keep a Changelog.
3. Убедитесь, что все локальные проверки пройдены:
   ```bash
   cargo fmt --check
   cargo clippy --all-targets -- -D warnings
   cargo test
   ```
4. Зафиксируйте изменения в Git:
   ```bash
   git commit -am "chore(release): bump version to vX.Y.Z"
   ```
5. Создайте аннотированный тег:
   ```bash
   git tag -a vX.Y.Z -m "Release vX.Y.Z"
   git push origin refs/tags/vX.Y.Z  # только один проверенный тег
   ```
6. Пайплайн GitHub Actions (`release.yml`) выполняется **только для тега вида `vX.Y.Z`**; перед сборкой проверяется соответствие версии в `Cargo.toml` и тега. Публикация разрешена только после успешной подготовки всех платформ:

   - Соберёт релизные бинарники под Windows, Linux, macOS (Apple Silicon и Intel).
   - Упакует архивы `.zip` и `.tar.gz` вместе со **всеми корневыми Markdown-руководствами** и `LICENSE`, чтобы ссылки из README работали офлайн.
   - Рассчитает контрольные суммы `SHA256SUMS.txt`.
   - Опубликует релиз на GitHub Releases с прикреплёнными архивами и чексуммами.

---

## 6. Контроль целостности и публикации

- Скачивайте пакеты только из [Releases](https://github.com/uzbatsu-beep/IP_PORT/releases), а не из сторонних сборок. Имя Windows-пакета: `ip_port_scanner-v0.4.0-x86_64-pc-windows-msvc.zip`.
- Для Linux/macOS можно проверить один файл командой `sha256sum <имя-архива>` или `shasum -a 256 <имя-архива>`, а затем сравнить с соответствующей строкой `SHA256SUMS.txt`. Проверка `sha256sum -c SHA256SUMS.txt` применима, когда рядом лежат **все** перечисленные архивы.
- SHA-256 помогает обнаружить повреждение/подмену относительно опубликованного манифеста, но **не заменяет криптографическую подпись издателя**. Текущий release workflow не выполняет Authenticode/Apple notarization или SLSA provenance.
- В workflow используются минимальные права на чтение для сборочных заданий; публикация требует `contents: write` только в отдельном job. Сборка на обычной ветке через `workflow_dispatch` не должна создавать релиз.
- После публикации проверьте четыре платформенных архива, `SHA256SUMS.txt`, тег и SHA коммита; архивы должен собирать именно релизный workflow. Чек-лист — [RELEASE_CHECKLIST.md](RELEASE_CHECKLIST.md).

## 7. Приёмка и устранение неполадок

- Запускайте Windows EXE сначала на тестовом ПК/изолированной сети; выполните тесты Services database, PTR, ARP/NDP и OUI по [TESTING.md](TESTING.md).
- При закрытии Windows-приложения откройте `Win+R` → `perfmon /rel` и запишите имя аварийного процесса, модуль и код исключения. `msedgewebview2.exe` может относиться к стороннему приложению — не следует приписывать эту ошибку native egui-сканеру без записи из системного журнала. Подробнее — [OPERATIONS.md](OPERATIONS.md).
- На macOS запуск неподписанного бинарника может потребовать явного разрешения в **Privacy & Security**. Не рекомендуем отключать Gatekeeper для всей системы; команду `xattr -cr` используйте только после проверки источника и хэша.


ENG-01.A: release builds use the exact Rust version in rust-toolchain.toml and `--locked`; see [ENGINEERING.md](ENGINEERING.md). This locks the Rust/dependency inputs, not the entire OS/linker environment.

Release and CI artifacts include [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) and the original bundled font license notices. CI artifacts may retain target/ paths because the upload root is now the repository root; locate ip_port_scanner.exe within the ZIP. No font binaries are added.

## Windows-only preview releases

The proposed [Windows Preview Release workflow](.github/workflows/windows-preview.yml) is manually dispatched on `main`. It checks latest exact-SHA main CI and the Windows job plus Quality Gate, builds the pinned locked x64 binary, packages the root documentation and notices, verifies ZIP and SHA-256, and publishes a prerelease under `preview-windows-vX.Y.Z-N`. It refuses existing tag names. This does not replace the stable four-platform release, which still requires cross-platform acceptance. Manual equipment testing remains pending. The stable Latest badge will remain on the last stable release.
