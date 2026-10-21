//! Простые zip-архивы для своих файлов кистей (`.gsbrush`, `.gsbrushes`) и плагинов: список
//! «имя в архиве — содержимое». Тот же zip, что у `.ora`.

use std::path::Path;

type Res<T> = Result<T, Box<dyn std::error::Error>>;

/// Записать архив (через временный файл: сбой не портит прежний).
pub fn write(path: &Path, entries: &[(String, Vec<u8>)]) -> Res<()> {
    paint_core::io::write_atomic(path, &crate::ora::zip(entries))?;
    Ok(())
}

/// Прочитать все файлы архива.
pub fn read(path: &Path) -> Res<Vec<(String, Vec<u8>)>> {
    crate::ora::unzip(&std::fs::read(path)?)
}

#[cfg(test)]
mod tests {
    #[test]
    fn round_trip() {
        let dir = std::env::temp_dir().join(format!("grazysai-pack-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("кисть.gsbrush");
        let entries = vec![("brush.txt".to_string(), b"name=x\n".to_vec()), ("tip.png".to_string(), vec![1, 2, 3, 250])];
        super::write(&p, &entries).unwrap();
        assert_eq!(super::read(&p).unwrap(), entries);
        std::fs::remove_dir_all(dir).ok();
    }
}
