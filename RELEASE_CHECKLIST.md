# Чек-лист выпуска IP / PORT Scanner

Используется **перед публикацией следующего тега**. Текущий официальный релиз — [v0.4.0](https://github.com/uzbatsu-beep/IP_PORT/releases/tag/v0.4.0), исходный релизный коммит `ed4f81b3ad938fe9d10e07d3165c91cb2be6e6e1`. Версия разработки 0.5.0 согласована; публикация тега требует отдельного согласования.

## 1. Готовность к выпуску

- [ ] В `main` находятся только прошедшие ревью PR; нет разовых workflow или тестовых файлов выпуска
- [ ] `Cargo.toml` содержит версию `X.Y.Z`; `Cargo.lock` синхронизирован
- [ ] `README.md`, `ROADMAP.md`, `ARCHITECTURE.md`, `SECURITY.md`, `PACKAGING.md` и `CHANGELOG.md` актуальны
- [ ] `rust-toolchain.toml` фиксирует Rust 1.100.0; полный preflight успешно пройден (format/docs, locked check/clippy/test/build); CI для точного SHA успешен
- [ ] `python3 scripts/check_docs.py` не находит несуществующих локальных ссылок
- [ ] Матричный CI Windows x64/Linux x64/macOS ARM64 и компиляция Rust 1.95 завершились `SUCCESS`
- [ ] Выполнен Windows smoke-test по [TESTING.md](TESTING.md), критичные сетевые/OT-дефекты исключены
- [ ] Версия/тег ещё не существуют или принято отдельное решение о корректирующем выпуске; опубликованные теги не переписываются

## 2. Тег и build

- [ ] Создан аннотированный тег `vX.Y.Z` **на проверенном commit SHA**, а не на произвольной release-ветке
- [ ] Тег соответствует `Cargo.toml`; `release.yml` запускается по тегу, не по обычной ветке
- [ ] Сборки прошли на Windows x64, Linux x64, macOS ARM64 и macOS Intel
- [ ] Все четыре архива присутствуют в GitHub Release:
  - `ip_port_scanner-vX.Y.Z-x86_64-pc-windows-msvc.zip`
  - `ip_port_scanner-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz`
  - `ip_port_scanner-vX.Y.Z-aarch64-apple-darwin.tar.gz`
  - `ip_port_scanner-vX.Y.Z-x86_64-apple-darwin.tar.gz`
- [ ] `SHA256SUMS.txt` содержит строку **каждого** опубликованного архива (а не только Windows ZIP)
- [ ] Контрольные суммы из manifest перепроверены по скачанным архивам
- [ ] Известные ограничения и улучшения документированы в release notes

## 3. После выпуска

- [ ] Проверена страница [Releases](https://github.com/uzbatsu-beep/IP_PORT/releases), ссылки и даты публикации
- [ ] Windows EXE стартует из скачанного ZIP; тест открытия Services database успешен
- [ ] Открыт отдельный Issue при выявлении дефекта (без секретов и конфиденциального адресного плана)
- [ ] Раздел `[Unreleased]` CHANGELOG содержит **только будущие**, ещё не выпущенные изменения
- [ ] Создан план для следующего исправления с приоритетами и критериями готовности

**Ограничения цепочки поставки:** SHA-256 проверяет целостность относительно опубликованного манифеста, но не подлинность издателя. Текущий процесс не включает кодовую подпись Windows, нотариальное заверение macOS или attestations; при необходимости они должны быть добавлены отдельным проектным этапом.

ENG-01.B: the release workflow requires the exact checked-out tag commit to have a successful latest CI push run on main and successful Quality Gate before any release build. Do not bypass the gate; wait for actual main CI success before creating an approved tag. Dependency audit and performance reports must be available.

- [ ] All four packages contain THIRD_PARTY_NOTICES.md and the original font notices under third-party/epaint_default_fonts; verify the bundled dependency version and remaining third-party license obligations before release.
