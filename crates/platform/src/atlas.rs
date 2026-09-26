use std::collections::HashMap;

use ui::{Coordinate, FontSize, Glyph, Point, Vector};
use wgpu::Extent3d;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub(crate) struct Texel(u32);

impl Texel {
    pub(crate) const ZERO: Self = Self(0);

    pub(crate) fn of_count(value: usize) -> Self {
        Self(u32::try_from(value).unwrap_or(0))
    }

    pub(crate) const fn get(self) -> u32 {
        self.0
    }

    pub(crate) fn count(self) -> usize {
        usize::try_from(self.0).unwrap_or(0)
    }

    pub(crate) fn float(self) -> f32 {
        Coordinate::of_integer(i32::try_from(self.0).unwrap_or(0)).get()
    }

    #[must_use]
    pub(crate) const fn plus(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }

    pub(crate) const fn square(self) -> Extent3d {
        Extent3d {
            width: self.0,
            height: self.0,
            depth_or_array_layers: 1,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Corner {
    pub(crate) left: Texel,
    pub(crate) top: Texel,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct Sprite {
    pub(crate) corner: Corner,
    pub(crate) width: Texel,
    pub(crate) height: Texel,
    pub(crate) offset: Point,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct SpriteKey {
    pub(crate) size: FontSize,
    pub(crate) glyph: Glyph,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Entry {
    Blank,
    Placed(Sprite),
}

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub(crate) struct Bitmap(Vec<u8>);

impl Bitmap {
    pub(crate) const fn new(pixels: Vec<u8>) -> Self {
        Self(pixels)
    }

    fn blank(side: Texel) -> Self {
        Self(vec![0; side.count() * side.count()])
    }

    pub(crate) fn as_slice(&self) -> &[u8] {
        &self.0
    }

    fn rows(&self, width: Texel) -> impl Iterator<Item = &[u8]> {
        self.0.chunks(width.count().max(1))
    }

    fn write(&mut self, offset: usize, row: &[u8]) {
        if let Some(target) = self.0.get_mut(offset..offset + row.len()) {
            target.copy_from_slice(row);
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Upload {
    Pending,
    Done,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Shelf {
    left: Texel,
    top: Texel,
    height: Texel,
}

pub(crate) struct Atlas {
    side: Texel,
    pixels: Bitmap,
    shelf: Shelf,
    pub(crate) upload: Upload,
    sprites: HashMap<SpriteKey, Entry>,
}

impl Atlas {
    pub(crate) const FIRST_SIDE: Texel = Texel(1024);
    const LARGEST_SIDE: Texel = Texel(8192);
    const WHITE_SIDE: Texel = Texel(4);
    const WHITE_RESERVED: Texel = Texel(5);

    pub(crate) fn new(side: Texel) -> Self {
        let mut pixels = Bitmap::blank(side);
        let white = [255; 4];
        for row in 0..Self::WHITE_SIDE.get() {
            pixels.write(Texel(row * side.get()).count(), &white);
        }
        Self {
            side,
            pixels,
            shelf: Shelf {
                left: Self::WHITE_RESERVED,
                top: Texel::ZERO,
                height: Self::WHITE_RESERVED,
            },
            upload: Upload::Pending,
            sprites: HashMap::new(),
        }
    }

    pub(crate) const fn side(&self) -> Texel {
        self.side
    }

    pub(crate) fn grown_side(&self) -> Texel {
        Texel(self.side.0 * 2).min(Self::LARGEST_SIDE)
    }

    pub(crate) const fn pixels(&self) -> &Bitmap {
        &self.pixels
    }

    pub(crate) fn white(&self) -> TextureArea {
        let side = self.side.float();
        TextureArea {
            start: Vector::new(Coordinate::new(1.0 / side), Coordinate::new(1.0 / side)),
            end: Vector::new(Coordinate::new(3.0 / side), Coordinate::new(3.0 / side)),
        }
    }

    pub(crate) fn area(&self, sprite: Sprite) -> TextureArea {
        let side = self.side.float();
        let corner = sprite.corner;
        TextureArea {
            start: Vector::new(
                Coordinate::new(corner.left.float() / side),
                Coordinate::new(corner.top.float() / side),
            ),
            end: Vector::new(
                Coordinate::new(corner.left.plus(sprite.width).float() / side),
                Coordinate::new(corner.top.plus(sprite.height).float() / side),
            ),
        }
    }

    pub(crate) fn cached(&self, key: SpriteKey) -> Option<Entry> {
        self.sprites.get(&key).copied()
    }

    pub(crate) fn remember(&mut self, key: SpriteKey, entry: Entry) {
        self.sprites.insert(key, entry);
    }

    pub(crate) fn allocate(&mut self, width: Texel, height: Texel) -> Option<Corner> {
        let width = width.plus(Texel(1));
        let height = height.plus(Texel(1));
        if self.shelf.left.plus(width) > self.side {
            self.shelf.top = self.shelf.top.plus(self.shelf.height);
            self.shelf.left = Texel::ZERO;
            self.shelf.height = Texel::ZERO;
        }
        if self.shelf.top.plus(height) > self.side {
            return None;
        }
        let corner = Corner {
            left: self.shelf.left,
            top: self.shelf.top,
        };
        self.shelf.left = self.shelf.left.plus(width);
        self.shelf.height = self.shelf.height.max(height);
        Some(corner)
    }

    pub(crate) fn blit(&mut self, corner: Corner, width: Texel, bitmap: &Bitmap) {
        let side = self.side.count();
        for (row, line) in bitmap.rows(width).enumerate() {
            let offset = (corner.top.count() + row) * side + corner.left.count();
            self.pixels.write(offset, line);
        }
        self.upload = Upload::Pending;
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct TextureArea {
    pub(crate) start: Vector,
    pub(crate) end: Vector,
}
