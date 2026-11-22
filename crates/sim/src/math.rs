use std::ops::{Add, Mul, Sub};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Número de ponto fixo usado em toda a simulação (32 bits inteiros, 32 fracionários).
pub type Fx = fixed::types::I32F32;

/// Serialização bit-a-bit determinística para `Fx` (salva os 64 bits inteiros exatos).
pub mod fxSerde {
    use super::Fx;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(val: &Fx, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_i64(val.to_bits())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Fx, D::Error> {
        let bits = i64::deserialize(d)?;
        Ok(Fx::from_bits(bits))
    }
}

/// Serialização bit-a-bit determinística para `Vec<Fx>`.
pub mod fxVecSerde {
    use super::Fx;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(vals: &[Fx], s: S) -> Result<S::Ok, S::Error> {
        let bits: Vec<i64> = vals.iter().map(|v| v.to_bits()).collect();
        bits.serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Fx>, D::Error> {
        let bits = Vec::<i64>::deserialize(d)?;
        Ok(bits.into_iter().map(Fx::from_bits).collect())
    }
}

/// Converte centésimos inteiros (ex.: `280` -> `2.80`) para `Fx` sem passar por float.
pub fn fxFromHundredths(value: i32) -> Fx {
    Fx::from_num(value) / Fx::from_num(100)
}

/// Vetor 2D em ponto fixo. Unidade: tiles. `x` cresce para leste, `y` para sul.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Vec2Fx {
    pub x: Fx,
    pub y: Fx,
}

impl Serialize for Vec2Fx {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        (self.x.to_bits(), self.y.to_bits()).serialize(s)
    }
}

impl<'de> Deserialize<'de> for Vec2Fx {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let (xb, yb) = <(i64, i64)>::deserialize(d)?;
        Ok(Self::new(Fx::from_bits(xb), Fx::from_bits(yb)))
    }
}

impl Vec2Fx {
    pub const ZERO: Self = Self { x: Fx::ZERO, y: Fx::ZERO };

    pub const fn new(x: Fx, y: Fx) -> Self {
        Self { x, y }
    }

    pub fn fromInts(x: i32, y: i32) -> Self {
        Self { x: Fx::from_num(x), y: Fx::from_num(y) }
    }

    pub fn tileCenter(tx: u32, ty: u32) -> Self {
        let half = Fx::ONE / Fx::from_num(2);
        Self::new(Fx::from_num(tx) + half, Fx::from_num(ty) + half)
    }

    pub fn lengthSquared(self) -> Fx {
        self.x * self.x + self.y * self.y
    }

    pub fn length(self) -> Fx {
        self.lengthSquared().sqrt()
    }

    pub fn distanceSquared(self, other: Self) -> Fx {
        (self - other).lengthSquared()
    }

    pub fn distance(self, other: Self) -> Fx {
        (self - other).length()
    }

    pub fn dot(self, other: Self) -> Fx {
        self.x * other.x + self.y * other.y
    }

    /// Vetor perpendicular girado 90° no plano XZ.
    pub fn perpendicular(self) -> Self {
        Self::new(-self.y, self.x)
    }

    pub fn normalizeOrZero(self) -> Self {
        let len = self.length();
        if len > Fx::ZERO {
            Self::new(self.x / len, self.y / len)
        } else {
            Self::ZERO
        }
    }

    /// Conversão só para apresentação; nunca realimentar o resultado na simulação.
    pub fn toF32(self) -> (f32, f32) {
        (self.x.to_num(), self.y.to_num())
    }
}

impl Add for Vec2Fx {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl Sub for Vec2Fx {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl Mul<Fx> for Vec2Fx {
    type Output = Self;
    fn mul(self, rhs: Fx) -> Self {
        Self::new(self.x * rhs, self.y * rhs)
    }
}
