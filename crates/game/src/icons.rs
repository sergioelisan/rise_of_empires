use bevy::asset::RenderAssetUsages;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

const ICON_SIZE: u32 = 24;

/// Catálogo de ícones procedurais em pixel-art 24x24 para recursos, ações e edificações.
#[derive(Resource)]
pub struct IconAtlas {
    pub food: Handle<Image>,
    pub wood: Handle<Image>,
    pub metal: Handle<Image>,
    pub wealth: Handle<Image>,
    pub knowledge: Handle<Image>,
    pub actMove: Handle<Image>,
    pub actAttack: Handle<Image>,
    pub actBuild: Handle<Image>,
    pub actRepair: Handle<Image>,
    buildings: HashMap<String, Handle<Image>>,
}

impl IconAtlas {
    pub fn build(images: &mut Assets<Image>) -> Self {
        let mut buildings = HashMap::new();
        buildings.insert("city_center".into(), images.add(drawCityCenter().toImage()));
        buildings.insert("farm".into(), images.add(drawFarm().toImage()));
        buildings.insert("lumber_camp".into(), images.add(drawLumberCamp().toImage()));
        buildings.insert("mine".into(), images.add(drawMine().toImage()));
        buildings.insert("market".into(), images.add(drawMarket().toImage()));
        buildings.insert("temple".into(), images.add(drawTemple().toImage()));
        buildings.insert("library".into(), images.add(drawUniversity().toImage()));
        buildings.insert("barracks".into(), images.add(drawBarracks().toImage()));
        buildings.insert("stable".into(), images.add(drawStable().toImage()));
        buildings.insert("siege_workshop".into(), images.add(drawSiegeWorkshop().toImage()));
        buildings.insert("tower".into(), images.add(drawTower().toImage()));
        buildings.insert("dock".into(), images.add(drawDock().toImage()));
        buildings.insert("airfield".into(), images.add(drawAirfield().toImage()));
        buildings.insert("wonder".into(), images.add(drawWonder().toImage()));

        Self {
            food: images.add(drawApple().toImage()),
            wood: images.add(drawPlank().toImage()),
            metal: images.add(drawIngot().toImage()),
            wealth: images.add(drawCoins().toImage()),
            knowledge: images.add(drawBook().toImage()),
            actMove: images.add(drawMove().toImage()),
            actAttack: images.add(drawAttack().toImage()),
            actBuild: images.add(drawHammer().toImage()),
            actRepair: images.add(drawRepair().toImage()),
            buildings,
        }
    }

    pub fn building(&self, id: &str) -> Handle<Image> {
        self.buildings
            .get(id)
            .cloned()
            .unwrap_or_else(|| self.actBuild.clone())
    }
}

struct Canvas {
    w: i32,
    h: i32,
    px: Vec<[u8; 4]>,
}

impl Canvas {
    fn new() -> Self {
        let s = ICON_SIZE as i32;
        Self { w: s, h: s, px: vec![[0, 0, 0, 0]; (s * s) as usize] }
    }

    fn set(&mut self, x: i32, y: i32, c: [u8; 4]) {
        if x >= 0 && y >= 0 && x < self.w && y < self.h {
            self.px[(y * self.w + x) as usize] = c;
        }
    }

    fn rect(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, c: [u8; 4]) {
        for y in y0..=y1 {
            for x in x0..=x1 {
                self.set(x, y, c);
            }
        }
    }

    fn circle(&mut self, cx: i32, cy: i32, r: i32, c: [u8; 4]) {
        let r2 = r * r;
        for y in (cy - r)..=(cy + r) {
            for x in (cx - r)..=(cx + r) {
                let dx = x - cx;
                let dy = y - cy;
                if dx * dx + dy * dy <= r2 {
                    self.set(x, y, c);
                }
            }
        }
    }

    fn line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, c: [u8; 4]) {
        let steps = (x1 - x0).abs().max((y1 - y0).abs()).max(1);
        for i in 0..=steps {
            let x = x0 + (x1 - x0) * i / steps;
            let y = y0 + (y1 - y0) * i / steps;
            self.set(x, y, c);
        }
    }

    /// Adiciona contorno escuro de 1 px ao redor dos pixels opacos para máxima legibilidade na HUD.
    fn outline(&mut self) {
        let orig = self.px.clone();
        for y in 0..self.h {
            for x in 0..self.w {
                if orig[(y * self.w + x) as usize][3] > 0 {
                    continue;
                }
                let touches = [(-1, 0), (1, 0), (0, -1), (0, 1)].iter().any(|&(dx, dy)| {
                    let nx = x + dx;
                    let ny = y + dy;
                    nx >= 0
                        && ny >= 0
                        && nx < self.w
                        && ny < self.h
                        && orig[(ny * self.w + nx) as usize][3] > 0
                });
                if touches {
                    self.set(x, y, [16, 18, 24, 225]);
                }
            }
        }
    }

    fn toImage(mut self) -> Image {
        self.outline();
        let mut data = Vec::with_capacity(self.px.len() * 4);
        for p in self.px {
            data.extend_from_slice(&p);
        }
        Image::new(
            Extent3d { width: ICON_SIZE, height: ICON_SIZE, depth_or_array_layers: 1 },
            TextureDimension::D2,
            data,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::RENDER_WORLD,
        )
    }
}

// ============================================================================
// ÍCONES DOS 5 RECURSOS
// ============================================================================

/// Alimento: Maçã vermelha com caule e folha verde.
fn drawApple() -> Canvas {
    let mut c = Canvas::new();
    let red = [214, 38, 42, 255];
    let darkRed = [156, 22, 28, 255];
    let shine = [252, 160, 154, 255];
    let stem = [108, 66, 30, 255];
    let leaf = [68, 178, 62, 255];

    c.circle(9, 13, 6, red);
    c.circle(15, 13, 6, red);
    c.rect(7, 13, 17, 18, red);
    c.circle(12, 16, 5, red);
    // Sombra inferior direita
    c.rect(14, 15, 18, 18, darkRed);
    c.rect(10, 19, 14, 20, darkRed);
    // Brilho
    c.rect(7, 10, 9, 12, shine);
    // Caule e folha
    c.rect(11, 4, 12, 8, stem);
    c.rect(13, 4, 17, 6, leaf);
    c.rect(14, 3, 18, 5, [104, 212, 84, 255]);
    c
}

/// Madeira: Tábua de madeira com veios e chanfro.
fn drawPlank() -> Canvas {
    let mut c = Canvas::new();
    let plank = [174, 114, 58, 255];
    let light = [208, 148, 84, 255];
    let dark = [122, 74, 34, 255];

    // Tábua em diagonal leve / horizontal clássica
    c.rect(3, 8, 20, 16, plank);
    c.rect(3, 8, 20, 10, light);
    c.rect(3, 15, 20, 16, dark);
    // Topo cortado à direita (anel de madeira)
    c.rect(18, 9, 20, 15, [146, 92, 44, 255]);
    // Veios horizontais
    c.line(5, 11, 16, 11, dark);
    c.line(6, 13, 17, 13, dark);
    c.line(4, 14, 12, 14, light);
    c
}

/// Ferro/Metal: Lingote metálico chanfrado.
fn drawIngot() -> Canvas {
    let mut c = Canvas::new();
    let top = [212, 222, 234, 255];
    let side = [132, 144, 160, 255];
    let dark = [84, 94, 108, 255];
    let glint = [250, 252, 255, 255];

    // Base do lingote
    c.rect(3, 13, 20, 18, side);
    c.rect(3, 16, 20, 18, dark);
    // Topo estreito brilhante
    c.rect(6, 8, 17, 13, top);
    c.rect(5, 10, 18, 13, top);
    // Brilho metálico
    c.rect(7, 9, 13, 10, glint);
    c.line(17, 9, 20, 15, dark);
    c
}

/// Riqueza: Pilha de moedas douradas.
fn drawCoins() -> Canvas {
    let mut c = Canvas::new();
    let gold = [242, 198, 52, 255];
    let light = [255, 234, 122, 255];
    let shade = [178, 130, 22, 255];

    // Pilha esquerda (3 moedas)
    for y in [16, 13, 10] {
        c.rect(4, y, 13, y + 2, shade);
        c.rect(5, y - 1, 12, y, gold);
        c.rect(6, y - 1, 9, y - 1, light);
    }
    // Pilha direita (4 moedas)
    for y in [18, 15, 12, 9] {
        c.rect(10, y, 19, y + 2, shade);
        c.rect(11, y - 1, 18, y, gold);
        c.rect(12, y - 1, 16, y - 1, light);
    }
    c
}

/// Conhecimento: Livro azul-real com páginas claras e fecho dourado.
fn drawBook() -> Canvas {
    let mut c = Canvas::new();
    let cover = [44, 96, 184, 255];
    let spine = [28, 62, 128, 255];
    let page = [242, 236, 214, 255];
    let gold = [238, 194, 58, 255];

    c.rect(5, 4, 19, 19, cover);
    c.rect(5, 4, 8, 19, spine);
    // Páginas aparecendo na lateral direita e base
    c.rect(17, 6, 19, 18, page);
    c.rect(8, 17, 19, 19, page);
    // Detalhes dourados na capa
    c.rect(10, 7, 15, 9, gold);
    c.rect(10, 12, 15, 13, gold);
    c
}

// ============================================================================
// ÍCONES DE AÇÕES (Mover, Atacar, Construir, Reformar)
// ============================================================================

fn drawMove() -> Canvas {
    let mut c = Canvas::new();
    let green = [54, 212, 96, 255];
    let light = [146, 246, 166, 255];

    // Seta larga apontando para a direita
    c.rect(4, 9, 13, 14, green);
    c.rect(4, 10, 13, 11, light);
    for i in 0..=6 {
        c.rect(13 + i, 5 + i, 13 + i, 18 - i, green);
    }
    c.line(13, 6, 18, 11, light);
    c
}

fn drawAttack() -> Canvas {
    let mut c = Canvas::new();
    let blade = [218, 226, 236, 255];
    let edge = [150, 162, 178, 255];
    let hilt = [234, 182, 48, 255];
    let grip = [178, 42, 42, 255];

    // Duas espadas cruzadas
    for off in -1..=1 {
        c.line(5 + off, 5, 18 + off, 18, if off == 0 { blade } else { edge });
        c.line(18 + off, 5, 5 + off, 18, if off == 0 { blade } else { edge });
    }
    // Guardas e punhos
    c.rect(4, 15, 9, 17, hilt);
    c.rect(14, 15, 19, 17, hilt);
    c.rect(4, 18, 6, 20, grip);
    c.rect(17, 18, 19, 20, grip);
    c
}

fn drawHammer() -> Canvas {
    let mut c = Canvas::new();
    let wood = [166, 106, 52, 255];
    let steel = [188, 198, 212, 255];
    let dark = [110, 120, 136, 255];

    // Cabo em diagonal
    for off in 0..=2 {
        c.line(5 + off, 19, 14 + off, 10, wood);
    }
    // Cabeça do martelo
    c.rect(10, 4, 19, 10, steel);
    c.rect(10, 8, 19, 10, dark);
    c.rect(11, 5, 18, 6, [238, 244, 250, 255]);
    c
}

fn drawRepair() -> Canvas {
    let mut c = Canvas::new();
    let gold = [232, 182, 54, 255];
    let wood = [156, 98, 48, 255];
    let green = [54, 216, 104, 255];

    // Martelo dourado à esquerda
    c.rect(6, 10, 8, 20, wood);
    c.rect(3, 5, 12, 10, gold);
    // Cruz verde de reparo à direita
    c.rect(15, 9, 17, 19, green);
    c.rect(11, 13, 21, 15, green);
    c
}

// ============================================================================
// ÍCONES DAS 12 EDIFICAÇÕES
// ============================================================================

fn drawCityCenter() -> Canvas {
    let mut c = Canvas::new();
    let wall = [214, 204, 182, 255];
    let roof = [186, 58, 48, 255];
    let door = [72, 48, 30, 255];
    let flag = [236, 190, 56, 255];

    // Torre esquerda
    c.rect(4, 7, 9, 19, wall);
    c.rect(3, 4, 10, 7, roof);
    // Salão central
    c.rect(9, 11, 19, 19, wall);
    c.rect(8, 8, 20, 11, roof);
    c.rect(13, 15, 15, 19, door);
    // Bandeira
    c.rect(16, 3, 17, 8, door);
    c.rect(17, 3, 20, 5, flag);
    c
}

fn drawFarm() -> Canvas {
    let mut c = Canvas::new();
    let soil = [124, 86, 42, 255];
    let wheat = [234, 192, 52, 255];
    let barn = [188, 66, 48, 255];
    let wall = [220, 204, 172, 255];

    // Fileiras de trigo
    c.rect(3, 12, 20, 20, soil);
    for y in [13, 16, 19] {
        c.rect(4, y, 19, y + 1, wheat);
    }
    // Celeiro no fundo
    c.rect(12, 7, 19, 12, wall);
    c.rect(11, 4, 20, 7, barn);
    c
}

fn drawLumberCamp() -> Canvas {
    let mut c = Canvas::new();
    let tree = [44, 138, 56, 255];
    let trunk = [118, 72, 36, 255];
    let steel = [212, 222, 232, 255];

    // Pinheiro à esquerda
    for i in 0..6 {
        c.rect(8 - i, 4 + i * 2, 8 + i, 6 + i * 2, tree);
    }
    c.rect(7, 16, 9, 20, trunk);
    // Machado à direita
    c.line(14, 19, 19, 8, trunk);
    c.rect(15, 7, 20, 12, steel);
    c
}

fn drawMine() -> Canvas {
    let mut c = Canvas::new();
    let rock = [128, 126, 124, 255];
    let dark = [24, 26, 32, 255];
    let wood = [156, 98, 46, 255];
    let ore = [210, 224, 240, 255];

    c.circle(12, 14, 8, rock);
    c.rect(4, 14, 20, 20, rock);
    // Túnel da mina + arco de madeira
    c.rect(8, 12, 16, 20, dark);
    c.rect(7, 10, 17, 12, wood);
    c.rect(7, 12, 8, 20, wood);
    c.rect(16, 12, 17, 20, wood);
    // Minério brilhando
    c.rect(10, 17, 14, 19, ore);
    c
}

fn drawMarket() -> Canvas {
    let mut c = Canvas::new();
    let red = [204, 52, 48, 255];
    let cream = [244, 234, 200, 255];
    let wood = [138, 86, 42, 255];
    let gold = [244, 200, 52, 255];

    // Toldo listrado
    for x in 3..=20 {
        let col = if (x / 3) % 2 == 0 { red } else { cream };
        c.rect(x, 5, x, 10, col);
    }
    // Postes e balcão
    c.rect(4, 11, 5, 19, wood);
    c.rect(18, 11, 19, 19, wood);
    c.rect(4, 15, 19, 19, wood);
    // Moedas no balcão
    c.rect(9, 12, 14, 15, gold);
    c
}

fn drawTemple() -> Canvas {
    let mut c = Canvas::new();
    let marble = [232, 228, 214, 255];
    let shade = [174, 168, 154, 255];
    let gold = [242, 198, 56, 255];

    // Degraus
    c.rect(3, 18, 20, 20, shade);
    // 4 colunas
    for x in [5, 9, 14, 18] {
        c.rect(x, 10, x + 1, 17, marble);
    }
    // Frontão triangular
    c.rect(3, 8, 20, 10, marble);
    c.rect(6, 6, 17, 8, marble);
    c.rect(9, 4, 14, 6, marble);
    // Chama/estrela dourada no topo
    c.rect(11, 2, 12, 4, gold);
    c
}

fn drawUniversity() -> Canvas {
    let mut c = Canvas::new();
    let wall = [226, 220, 204, 255];
    let dome = [46, 104, 194, 255];
    let gold = [240, 198, 58, 255];

    c.rect(4, 11, 19, 19, wall);
    c.circle(12, 10, 6, dome);
    c.rect(11, 3, 12, 5, gold);
    // Colunas
    for x in [6, 11, 16] {
        c.rect(x, 13, x + 1, 19, [168, 162, 148, 255]);
    }
    c
}

fn drawBarracks() -> Canvas {
    let mut c = Canvas::new();
    let shield = [186, 44, 46, 255];
    let rim = [230, 188, 62, 255];
    let spear = [208, 216, 226, 255];

    // Lanças cruzadas atrás
    c.line(4, 4, 19, 19, spear);
    c.line(19, 4, 4, 19, spear);
    // Escudo central
    c.rect(7, 6, 16, 14, rim);
    c.rect(8, 7, 15, 13, shield);
    c.rect(9, 14, 14, 17, shield);
    c.rect(11, 17, 12, 19, rim);
    c
}

fn drawStable() -> Canvas {
    let mut c = Canvas::new();
    let roof = [144, 78, 38, 255];
    let gold = [234, 192, 66, 255];
    let dark = [90, 52, 26, 255];

    c.rect(3, 5, 20, 8, roof);
    c.rect(4, 9, 19, 19, dark);
    // Ferradura dourada no centro
    c.rect(7, 10, 16, 12, gold);
    c.rect(7, 11, 9, 17, gold);
    c.rect(14, 11, 16, 17, gold);
    c
}

fn drawSiegeWorkshop() -> Canvas {
    let mut c = Canvas::new();
    let wood = [166, 104, 48, 255];
    let iron = [184, 194, 208, 255];

    // Roda de madeira com aro de ferro
    c.circle(12, 13, 7, iron);
    c.circle(12, 13, 5, wood);
    c.circle(12, 13, 2, iron);
    c.line(12, 6, 12, 20, iron);
    c.line(5, 13, 19, 13, iron);
    c
}

fn drawTower() -> Canvas {
    let mut c = Canvas::new();
    let stone = [164, 166, 172, 255];
    let dark = [98, 102, 110, 255];
    let flag = [204, 52, 48, 255];

    // Ameias
    c.rect(6, 5, 8, 8, stone);
    c.rect(10, 5, 13, 8, stone);
    c.rect(15, 5, 17, 8, stone);
    // Corpo da torre
    c.rect(7, 8, 16, 20, stone);
    c.rect(14, 8, 16, 20, dark);
    // Seteira
    c.rect(11, 11, 12, 15, [28, 30, 36, 255]);
    c.rect(11, 2, 15, 4, flag);
    c
}

fn drawDock() -> Canvas {
    let mut c = Canvas::new();
    let water = [44, 128, 212, 255];
    let foam = [154, 212, 248, 255];
    let iron = [196, 206, 220, 255];
    let gold = [238, 194, 56, 255];

    // Ondas azuis na base
    c.rect(3, 16, 20, 20, water);
    c.rect(4, 16, 8, 17, foam);
    c.rect(14, 16, 19, 17, foam);
    // Âncora náutica central
    c.circle(12, 5, 2, gold);
    c.rect(11, 7, 12, 16, iron);
    c.rect(7, 9, 16, 10, iron);
    c.rect(6, 14, 17, 16, iron);
    c.rect(5, 12, 7, 15, iron);
    c.rect(16, 12, 18, 15, iron);
    c
}

fn drawAirfield() -> Canvas {
    let mut c = Canvas::new();
    let sky = [76, 156, 224, 255];
    let wing = [238, 232, 210, 255];
    let red = [204, 54, 48, 255];
    let gold = [242, 198, 56, 255];

    // Fundo de céu leve circular
    c.circle(12, 12, 8, sky);
    // Asas abertas do biplano/dirigível
    c.rect(3, 9, 20, 12, wing);
    c.rect(5, 13, 18, 14, [186, 180, 164, 255]);
    // Fuselagem central e leme
    c.rect(10, 5, 13, 18, red);
    c.rect(8, 16, 15, 18, red);
    // Hélice dourada na ponta
    c.rect(9, 4, 14, 5, gold);
    c
}

fn drawWonder() -> Canvas {
    let mut c = Canvas::new();
    let sand = [220, 184, 92, 255];
    let shade = [174, 138, 56, 255];
    let gold = [255, 224, 88, 255];

    for step in 0..7 {
        let y = 19 - step * 2;
        c.rect(3 + step, y - 1, 20 - step, y, sand);
        c.rect(12, y - 1, 20 - step, y, shade);
    }
    c.rect(10, 3, 13, 6, gold);
    c
}
