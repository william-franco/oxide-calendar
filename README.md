# Oxide Calendar

Calendário desktop para Linux, local-first, em Rust + Slint, com o design system [oxide-ui](https://github.com/william-franco/oxide-ui) (Material 3). Todos os dados ficam no seu computador, em SQLite.

## Funcionalidades

- Visualizações: mês, semana (7, 5 ou 3 dias conforme a largura), dia e agenda dos próximos 60 dias.
- Eventos: criar, editar, duplicar e excluir, com título, descrição, local, início, fim, dia inteiro, categoria, repetição e lembrete.
- Recorrência: diária, semanal, mensal e anual. Ao editar ou excluir uma ocorrência de uma série, você escolhe entre só esta ou toda a série.
- Categorias com cor própria, que podem ser exibidas ou ocultadas.
- Busca por texto (ignora maiúsculas e acentos) em todas as visualizações.
- Importação e exportação de arquivos `.ics`.
- Lembretes com notificação nativa, enquanto o app estiver aberto.
- Tema claro e escuro, pelo botão na barra superior ou seguindo o sistema na abertura.

## Atalhos

| Tecla | Ação |
|---|---|
| `←` / `→` (ou `PgUp` / `PgDn`) | Período anterior / seguinte |
| `T` | Ir para hoje |
| `M` `W` `D` `A` | Mês, semana, dia, agenda |
| `Ctrl+N` | Novo evento |
| `Esc` | Fechar editor ou menu |

## Layout responsivo

- Até 599 px: categorias e mini-calendário ficam num painel lateral aberto pelo ícone de menu.
- De 600 a 839 px: semana de 5 dias.
- A partir de 840 px: barra lateral fixa e semana de 7 dias.

## Compilar e executar (Linux)

Dependências de sistema:

```
sudo apt install libfontconfig1-dev libxkbcommon-dev libwayland-dev libdbus-1-dev
```

```
cargo run --release
cargo test
```

O banco fica em `$XDG_DATA_HOME/oxide-calendar/calendar.db` (padrão: `~/.local/share/oxide-calendar/calendar.db`).

## Arquitetura

```
src/domain/     datas, recorrência (RFC 5545, subconjunto), eventos, layout de sobreposição, busca
src/ics/        leitura e escrita de .ics
src/storage/    SQLite (rusqlite) com migrações por PRAGMA user_version
src/viewmodel/  estado da tela, montagem das visualizações e editor
src/bridge.rs   liga callbacks do Slint ao ViewModel
ui/             telas Slint (janela, visualizações, tipos)
migrations/     esquema SQL
```

## Limitações conhecidas

- Não há arrastar para criar evento: clicar em um horário cria um evento de 1 hora nesse horário.
- Não há atalho de busca (`Ctrl+F`) nem diálogo de confirmação ao excluir.
- Editar uma só ocorrência cancela essa ocorrência e cria um evento avulso com os novos dados. A série continua igual nas demais datas.
- Fusos horários: horários com `Z` são convertidos para o fuso local. Horários com `TZID` são lidos como horário local.
- Importar o mesmo arquivo duas vezes cria cópias dos eventos.
- Lembretes só disparam com o app aberto.

## License

MIT License. Copyright (c) 2026 William Franco.
