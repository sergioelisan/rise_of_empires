# Rise of Empires

RTS histórico original em **Rust + Bevy 0.20**, inspirado nos clássicos do gênero (fronteiras nacionais, cidades como âncora da economia, eras históricas, atrito em território inimigo e maravilhas). Toda a arte (texturas procedurais, modelos low-poly, ícones pixel-art 24×24 e efeitos sonoros WAV) é gerada em código, e a simulação é 100% determinística em ponto fixo (`I32F32`).

## Funcionalidades implementadas

- **Mapa procedural determinístico:** relevo, lagos com margem/espuma, praias, planícies, florestas low-poly, colinas e montanhas com iluminação direcional e sombras.
- **Unidades, *flow fields* e formações:** seleção por clique e caixa de arrasto, pathfinding por *flow fields* com cache, formações (`Caixa`, `Linha`, `Cunha`), modelos low-poly por classe e animações procedurais de marcha/trabalho/combate.
- **Economia e dados em RON (`data/`):** Alimento, Madeira, Ferro, Riqueza e Conhecimento; fazendeiros trabalham dentro das fazendas (1 trabalhador, 3 fazendas/cidade), lenhadores espaçados cortam na floresta e entregam toras na madeireira (1 a 5 vagas conforme a mata próxima), mercadores trabalham dentro do pátio do mercado (1/cidade), Templo (1/cidade) e Universidade (1/cidade na Era Clássica), com limite global de coleta por cidade (*Commerce Cap*) e reparo/reforma de construções.
- **Território e combate:** cidades que projetam e expandem fronteiras nacionais, atrito em solo inimigo, triângulo de *counters* (`Infantaria` > `Cavalaria` > `Arqueiros` > `Infantaria`; `Cerco` 250% contra prédios), captura de cidades ao zerar o HP e névoa de guerra.
- **Progressão e conteúdo:** 4 eras (`Antiguidade`, `Era Clássica`, `Idade Média`, `Era da Pólvora`) com troca visual automática de unidades e construções, árvore de 12 pesquisas (4 ramos), 5 civilizações históricas (`Roma`, `Egito`, `Grécia`, `Pérsia`, `China`) com bônus e unidades únicas, e 5 Maravilhas globais.
- **IA e acabamento:** oponente IA determinístico (`Passiva`, `Fácil`, `Normal`, `Difícil`), menu principal e pausa, minimapa interativo, HUD com ícones e tooltips, som procedural e save/load determinístico bit-a-bit em RON.

## Como executar

Requisitos: Rust (edition 2024) e suporte a Vulkan/GPU. No Linux, os efeitos sonoros usam `pw-play` (PipeWire) ou `aplay` quando disponíveis.

```bash
# Abrir o jogo (Menu Principal)
cargo run

# Entrar direto em uma partida personalizada
ROE_QUICKSTART=1 ROE_NATION=roma ROE_ENEMY=grecia ROE_AI=normal ROE_SEED=42 cargo run

# Rodar todos os testes automatizados da simulação determinística
cargo test --workspace
```

## Controles principais

| Comando | Ação |
|---------|------|
| `WASD` / Setas / borda da tela | Mover câmera |
| Roda do mouse | Zoom |
| Clique esquerdo / arrastar | Selecionar unidade, grupo (caixa) ou construção |
| Clique direito | Ordem contextual (mover, atacar, coletar, construir/reformar) |
| Clique no minimapa | Mover câmera (esquerdo) / enviar tropas (direito) |
| `T` / `W` | Abrir Pesquisas & Eras / Maravilhas |
| `G` | Alternar formação (`Caixa` / `Linha` / `Cunha`) |
| `F` / `M` | Alternar Névoa de Guerra / Som |
| `H` / `E` | Focar Capital aliada / Capital inimiga |
| `F5` / `F9` / `F10` | Salvar rápido / Carregar rápido / Menu de pausa |

## Arquitetura do workspace

- `crates/sim` (`roeSim`) — núcleo 100% determinístico (sem Bevy, sem `f32`/`f64`, sem iteração de `HashMap`), guiado exclusivamente por `Command`s com `checksum()` para validação de determinismo e save/load bit-a-bit.
- `crates/render` (`roeRender`) — apresentação 3D Bevy, câmera isométrica 2:1, malhas low-poly procedurais, terreno, fronteiras/névoa, minimapa interativo e entrada.
- `crates/game` (`roeGame`) — executável, menus, HUD com ícones/tooltips e síntese procedural de áudio.
- `data/` — definições de unidades, construções, tecnologias, eras, civilizações e maravilhas em RON.

## Licença

Distribuído sob a licença **MIT**. Veja [LICENSE](LICENSE).
