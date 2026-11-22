# Rise of Empires — plano de implementação

Atualizado em 2026-11-22.

RTS histórico original, inspirado nos clássicos do gênero (fronteiras nacionais, cidades como
âncora da economia, eras, atrito em território inimigo, maravilhas). Não é um clone de nenhum
jogo existente: nome, nações, unidades, arte, interface e mapas são próprios.

## Decisões tomadas

| Tema | Decisão |
|------|---------|
| Linguagem / engine | Rust + Bevy 0.20 |
| Visual | Low-poly com câmera isométrica de ângulo fixo (tiles 2:1), sensação de RTS clássico |
| Arte | Toda feita por nós: texturas procedurais em código, modelos por script/malha gerada, ícones em SVG |
| Nações | Civilizações históricas reais, com bônus e unidades de desenho próprio |
| Multiplayer | No futuro; a simulação já é determinística (ponto fixo, só muda por comandos) para permitir lockstep |
| Dados | Unidades, construções, tecnologias e nações em arquivos RON (a partir da fase 3) |
| Estilo de código | Tipos em `PascalCase`, funções e variáveis em `camelCase` (`non_snake_case` liberado no workspace) |

## Estado atual

**Fases 0 a 9 concluídas.** `cargo run` abre no Menu Principal interativo com escolha de
civilização, oponente, dificuldade da IA (`Passiva`, `Facil`, `Normal`, `Dificil`), semente do mapa
e carregamento de saves, entrando em uma partida completa contra uma IA ativa determinística, com
minimapa interativo, HUD completa, efeitos sonoros procedurais e save/load determinístico em RON.

- `data/` — definições em RON (`units.ron`, `buildings.ron`, `techs.ron`, `nations.ron`).
- `crates/sim` — simulação 100% determinística, sem Bevy e sem `f32`:
  - `map.rs` — geração procedural em ponto fixo (`TerrainType`, `heightAt`, passabilidade);
  - `data.rs` — carregamento dos arquivos RON (com fallback embutido para testes);
  - `pathfinding.rs` — *flow fields* com cache determinístico e formações (`Box`, `Line`, `Wedge`);
  - `territory.rs` — cidades, mapa de fronteiras nacionais e névoa de guerra (`Unexplored`/`Explored`/`Visible`);
  - `combat.rs` — matriz de *counters* (infantaria, cavalaria, arqueiros, cerco) e eventos de projéteis;
  - `ai.rs` — IA determinística (`AiDifficulty`) que gerencia economia, construções, expansão de
    cidades, pesquisas, eras, maravilhas, defesa territorial e ondas de ataque;
  - `save.rs` — serialização/desserialização determinística bit-a-bit (`Fx` em `i64`) em RON (`saves/quicksave.ron`);
  - `world.rs` — economia com limite de coleta por cidade, fazendeiros dentro da lavoura, lenhadores
    espaçados cortando na floresta e entregando na madeireira, captura de cidades, atrito, 4 eras,
    12 pesquisas, 5 civilizações e 5 maravilhas.
  - **24 testes passando** (`cargo test --workspace`).
- `crates/render` — câmera isométrica (`camera.rs`), terreno + água + árvores/rochas + overlay
  dinâmico de fronteiras e névoa de guerra (`terrain.rs`), modelos low-poly procedurais que evoluem
  por era + unidades únicas + 5 maravilhas (`models.rs`), unidades com animações de trabalho/marcha,
  partículas, anéis, barras de vida e projéteis (`units.rs`), construções com andaimes e barras
  (`buildings.rs`), seleção por clique/caixa, ordens contextuais e prévia fantasma (`input.rs`),
  e minimapa interativo com clique/arrasto de câmera e movimento (`minimap.rs`).
- `crates/game` — menu principal e pausa (`menu.rs`), HUD completa com toasts/gavetas (`hud.rs`),
  síntese procedural de efeitos sonoros WAV tocados via PipeWire/ALSA (`audio.rs`) e executável (`main.rs`).

Atalhos e variáveis úteis:
- `F5` / `F9`: Salvar / Carregar rápido (`saves/quicksave.ron`); `F10` ou `Esc`: Menu de Pausa;
- `T`: Pesquisas & Eras; `W`: Maravilhas; `F`: Névoa de Guerra; `M`: Mudo/Som; `G`: Formação; `H`/`E`: Capital/Inimigo;
- `ROE_QUICKSTART=1 ROE_NATION=roma ROE_ENEMY=grecia ROE_AI=normal ROE_SEED=42 cargo run`
- `ROE_SCREENSHOT=arquivo.png cargo run` salva um quadro e encerra.

### Pendências acumuladas

- [x] Revisão final das fases 8 e 9 (IA ativa, minimapa, som, menus, save/load, ícones, tooltips, limites de trabalhadores/cidades e Templo).
- [x] Sombras das unidades não aparecem no chão (corrigido na fase 1).
- [x] Som implementado por síntese procedural WAV reproduzida via `pw-play`/`aplay` (sem exigir `libasound2-dev`).
- [x] Repositório publicado no GitHub sob licença MIT.
- [ ] Fase 10 (opcional): naval e aéreo, multiplayer lockstep, campanha estratégica.

## Fases

Cada fase termina em algo jogável. As fases 0 a 5 formam o primeiro marco: uma partida de uma
era só contra um inimigo passivo.

- [x] **0 — Fundação:** workspace, janela, câmera isométrica, loop de tick fixo.
- [x] **1 — Mapa:** terreno procedural (relevo, água, florestas, montanhas) com texturas
      procedurais; tipos de terreno na simulação (passável ou não); iluminação e sombras.
- [x] **2 — Unidades:** seleção (clique e caixa), ordens de movimento, pathfinding (flow
      fields), formações, primeiros modelos low-poly.
- [x] **3 — Economia:** cidadãos, coleta de recursos, construções, limite de coleta por cidade;
      definições em RON.
- [x] **4 — Território:** cidades, fronteiras que crescem, atrito em solo inimigo.
- [x] **5 — Combate:** counters entre infantaria, cavalaria, arqueiros e cerco; captura de
      cidades; névoa de guerra.
- [x] **6 — Progressão:** árvore de pesquisa e eras, com troca de visual e de unidades.
- [x] **7 — Conteúdo:** 4 a 6 civilizações históricas com bônus e unidades próprias; maravilhas.
- [x] **8 — IA:** oponente com economia, expansão e ataques.
- [x] **9 — Acabamento:** HUD completa, minimapa, som, menus, save/load.
- [ ] **10 — Opcional:** naval e aéreo, multiplayer lockstep, campanha de conquista em mapa
      estratégico.

## Próximo passo

Revisar as fases 8 e 9 ao vivo (IA ativa, minimapa, menus, som e save/load) e, se desejado,
escolher quais itens da **Fase 10 — Opcional** atacar (naval/aéreo, multiplayer lockstep ou
campanha estratégica).

## Regras a manter

- Nada de `f32`/`f64`, relógio, aleatoriedade externa ou iteração de `HashMap` dentro de
  `crates/sim`.
- A apresentação nunca altera a simulação diretamente: só enfileira `Command`s.
- Conversões para `f32` (`toF32`) servem só para desenhar; nunca voltam para a simulação.
