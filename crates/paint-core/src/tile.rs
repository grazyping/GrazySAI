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
}

pub type ColorMap = TileMap<Px>;
pub type MaskMap = TileMap<u16>;

pub const EMPTY_PX: Px = CLEAR;
