//! Разреженное хранение: холст из плиток 64×64, пустые плитки не занимают память.

use crate::pixel::{Px, CLEAR};
use std::collections::HashMap;

pub const TILE: i32 = 64;
pub const TILE_LEN: usize = (TILE * TILE) as usize;
pub type Key = (i32, i32);

pub fn key_of(x: i32, y: i32) -> Key {
    (x.div_euclid(TILE), y.div_euclid(TILE))
}

pub fn index_of(x: i32, y: i32) -> usize {
    (y.rem_euclid(TILE) * TILE + x.rem_euclid(TILE)) as usize
}

/// Плитка значений типа `T` (цвет слоя или покрытие штриха).
#[derive(Clone, PartialEq, Debug)]
pub struct Tile<T: Copy>(pub Box<[T]>);

impl<T: Copy> Tile<T> {
    pub fn filled(v: T) -> Self {
        Tile(vec![v; TILE_LEN].into_boxed_slice())
    }
}

#[derive(Clone, Default, PartialEq, Debug)]
pub struct TileMap<T: Copy + Default> {
    pub tiles: HashMap<Key, Tile<T>>,
}

impl<T: Copy + Default> TileMap<T> {
    pub fn new() -> Self {
        Self { tiles: HashMap::new() }
    }

    pub fn get(&self, x: i32, y: i32) -> T {
        match self.tiles.get(&key_of(x, y)) {
            Some(t) => t.0[index_of(x, y)],
            None => T::default(),
        }
    }

    pub fn tile_mut(&mut self, key: Key) -> &mut Tile<T> {
        self.tiles.entry(key).or_insert_with(|| Tile::filled(T::default()))
    }

    pub fn set(&mut self, x: i32, y: i32, v: T) {
        self.tile_mut(key_of(x, y)).0[index_of(x, y)] = v;
    }

    /// Прямоугольник `w`×`h` от (`x0`, `y0`) построчно; где плиток нет — `T::default()`. Копирует
    /// строками плиток, а не поиском плитки на каждую точку.
    pub fn read_rect(&self, x0: i32, y0: i32, w: usize, h: usize) -> Vec<T> {
        let mut out = vec![T::default(); w * h];
        if w == 0 || h == 0 {
            return out;
        }
        let (x1, y1) = (x0 + w as i32, y0 + h as i32);
        let (k0, k1) = (key_of(x0, y0), key_of(x1 - 1, y1 - 1));
        for ky in k0.1..=k1.1 {
            for kx in k0.0..=k1.0 {
                let Some(t) = self.tiles.get(&(kx, ky)) else { continue };
                let (tx, ty) = (kx * TILE, ky * TILE);
                let (xs, xe) = (x0.max(tx), x1.min(tx + TILE));
                for y in y0.max(ty)..y1.min(ty + TILE) {
                    let row = ((y - ty) * TILE) as usize;
                    let src = &t.0[row + (xs - tx) as usize..row + (xe - tx) as usize];
                    let o = (y - y0) as usize * w + (xs - x0) as usize;
                    out[o..o + src.len()].copy_from_slice(src);
                }
            }
        }
        out
    }

    /// Записать прямоугольник `w`×`h` от (`x0`, `y0`) из построчного буфера (обратное к
    /// `read_rect`); недостающие плитки создаются.
    pub fn write_rect(&mut self, x0: i32, y0: i32, w: usize, h: usize, data: &[T]) {
        if w == 0 || h == 0 {
            return;
        }
        let (x1, y1) = (x0 + w as i32, y0 + h as i32);
        let (k0, k1) = (key_of(x0, y0), key_of(x1 - 1, y1 - 1));
        for ky in k0.1..=k1.1 {
            for kx in k0.0..=k1.0 {
                let t = self.tile_mut((kx, ky));
                let (tx, ty) = (kx * TILE, ky * TILE);
                let (xs, xe) = (x0.max(tx), x1.min(tx + TILE));
                for y in y0.max(ty)..y1.min(ty + TILE) {
                    let row = ((y - ty) * TILE) as usize;
                    let o = (y - y0) as usize * w + (xs - x0) as usize;
                    t.0[row + (xs - tx) as usize..row + (xe - tx) as usize].copy_from_slice(&data[o..o + (xe - xs) as usize]);
                }
            }
        }
    }
}

pub type ColorMap = TileMap<Px>;
pub type MaskMap = TileMap<u16>;

pub const EMPTY_PX: Px = CLEAR;
