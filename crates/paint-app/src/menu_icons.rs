//! Значки пунктов меню — шрифт Phosphor (MIT, `egui-phosphor`): знак перед подписью, как в
//! привычных программах (открыть — папка, сохранить — дискета…). Подпись — ключ перевода;
//! пункт без своего знака получает пустое место той же ширины, чтобы подписи шли ровно.

use crate::i18n::tr;
use eframe::egui::{self, text::LayoutJob, Color32, FontId, TextFormat};
use egui_phosphor::regular as ph;
use std::sync::Mutex;

/// Шрифт кнопок (из стиля, обновляется каждый кадр — масштаб интерфейса меняется).
static FONT: Mutex<Option<FontId>> = Mutex::new(None);

pub fn set_font(ctx: &egui::Context) {
    *FONT.lock().unwrap() = Some(egui::TextStyle::Button.resolve(&ctx.style()));
}

/// Знак пункта меню по ключу подписи.
pub fn icon(key: &str) -> Option<&'static str> {
    Some(match key {
        // Файл
        "Создать холст…" => ph::FILE_PLUS,
        "Открыть…" => ph::FOLDER_OPEN,
        "Сохранить" => ph::FLOPPY_DISK,
        "Сохранить как…" => ph::FLOPPY_DISK_BACK,
        "Сохранить с номером" => ph::LIST_NUMBERS,
        "Сохранить набор кистей…" => ph::PAINT_BRUSH_BROAD,
        "Водяной знак…" => ph::STAMP,
        "Открыть через обозреватель…" => ph::FOLDERS,
        "Сохранить через обозреватель…" => ph::FOLDER_SIMPLE_PLUS,
        "Экспорт" => ph::EXPORT,
        "Недавние" => ph::CLOCK_COUNTER_CLOCKWISE,
        "Очистить список" => ph::BROOM,
        "Закрыть" => ph::X,
        "Холст из буфера обмена" => ph::CLIPBOARD,
        "Выход" => ph::SIGN_OUT,
        // Правка
        "Отменить" => ph::ARROW_COUNTER_CLOCKWISE,
        "Повторить" => ph::ARROW_CLOCKWISE,
        "Вырезать" => ph::SCISSORS,
        "Копировать" => ph::COPY,
        "Вставить" => ph::CLIPBOARD_TEXT,
        "Копировать по границам выделения" => ph::COPY_SIMPLE,
        // Холст
        "Изменить разрешение…" => ph::ARROWS_OUT,
        "Изменить размер…" => ph::RESIZE,
        "Перевести в оттенки серого…" => ph::CIRCLE_HALF,
        "Отразить по горизонтали" | "Отразить слой по горизонтали" => ph::FLIP_HORIZONTAL,
        "Отразить по вертикали" | "Отразить слой по вертикали" => ph::FLIP_VERTICAL,
        "Повернуть на 90° против часовой" | "Повернуть слой на 90° против часовой" | "Повернуть вид против часовой" => ph::ARROW_COUNTER_CLOCKWISE,
        "Повернуть на 90° по часовой" | "Повернуть слой на 90° по часовой" | "Повернуть вид по часовой" => ph::ARROW_CLOCKWISE,
        "Обрезать по выделению" => ph::CROP,
        "Фон холста" => ph::CHECKERBOARD,
        "Цвет фона холста…" => ph::PAINT_BUCKET,
        "Цвет эффектов слоя" => ph::PALETTE,
        // Слой
        "Новый слой" => ph::FILE_PLUS,
        "Новый слой контура" => ph::PEN_NIB,
        "Новый слой фигур" => ph::SHAPES,
        "Новая папка" => ph::FOLDER_PLUS,
        "Новый слой 3D…" => ph::CUBE,
        "Новый слой коррекции" => ph::CIRCLE_HALF_TILT,
        "Стиль и коррекция слоя…" => ph::DROP_HALF,
        "Окно 3D" => ph::CUBE_FOCUS,
        "Копировать слой" | "Копия" => ph::COPY,
        "Удалить слой" | "Удалить" | "Удалить группу" => ph::TRASH,
        "Очистить слой" => ph::ERASER,
        "Перенести на нижний" => ph::ARROW_LINE_DOWN,
        "Слить с нижним" => ph::ARROWS_MERGE,
        "Поднять слой" => ph::ARROW_UP,
        "Опустить слой" => ph::ARROW_DOWN,
        "Слой на передний план" => ph::ARROW_LINE_UP,
        "Слой на задний план" => ph::ARROW_LINE_DOWN,
        "Показать только этот слой" => ph::EYE,
        "Новая маска слоя" => ph::SELECTION_PLUS,
        "Применить маску" => ph::SELECTION_FOREGROUND,
        "Удалить маску" => ph::SELECTION_SLASH,
        "Раскрасить по контуру" => ph::PAINT_ROLLER,
        "Растеризовать слой" => ph::GRID_FOUR,
        "Обводка…" => ph::PENCIL_CIRCLE,
        "Яркость в прозрачность" => ph::DROP_HALF,
        "Слить видимые" => ph::STACK,
        "Свести всё" => ph::STACK_SIMPLE,
        "Свойства слоя…" | "Свойства…" => ph::SLIDERS_HORIZONTAL,
        // Выделение
        "Выделить всё" => ph::SELECTION_ALL,
        "Снять выделение" | "Снять выделение узлов" => ph::SELECTION_SLASH,
        "Инвертировать" => ph::SELECTION_INVERSE,
        "Скрыть выделение" => ph::EYE_SLASH,
        "Показать выделение" => ph::EYE,
        "Расширить выделение…" | "Расширить на 1 px" => ph::ARROWS_OUT_SIMPLE,
        "Сузить выделение…" | "Сузить на 1 px" => ph::ARROWS_IN_SIMPLE,
        "Выделить узлы в выделении" => ph::BEZIER_CURVE,
        "Выделить штрихи по выделению" => ph::SCRIBBLE,
        "Область по непрозрачности слоя" => ph::SELECTION_BACKGROUND,
        // Линейка
        "Показать линейку" => ph::RULER,
        "Симметрия" => ph::FLIP_HORIZONTAL,
        "Не замечать линейку" => ph::PROHIBIT,
        "Сбросить линейку" => ph::ARROW_COUNTER_CLOCKWISE,
        // Фильтр
        "Тон и насыщенность…" => ph::DROP,
        "Яркость и контраст…" => ph::SUN,
        "Размытие" => ph::DROP_HALF_BOTTOM,
        "Эффекты" => ph::SPARKLE,
        "Искажение" => ph::WAVES,
        "Отрисовка" => ph::CLOUD,
        "Коррекция" => ph::FADERS,
        "Резкость" => ph::TRIANGLE,
        "Плагины" => ph::PUZZLE_PIECE,
        // Вид
        "Увеличить" => ph::MAGNIFYING_GLASS_PLUS,
        "Уменьшить" => ph::MAGNIFYING_GLASS_MINUS,
        "Масштаб 100%" => ph::MAGNIFYING_GLASS,
        "Вписать в окно" => ph::CORNERS_OUT,
        "Сбросить положение вида" => ph::CROSSHAIR,
        "Сбросить угол вида" => ph::COMPASS,
        "Новый вид" => ph::BROWSERS,
        "Новый плавающий вид" => ph::PICTURE_IN_PICTURE,
        "Предыдущий вид" => ph::CARET_LEFT,
        "Следующий вид" => ph::CARET_RIGHT,
        "Закрыть все виды" => ph::X_SQUARE,
        // Окно
        "Библиотека кистей…" => ph::BOOKS,
        "Менеджер ресурсов…" => ph::ARCHIVE,
        "Автодействия…" => ph::PLAY_CIRCLE,
        "Показать" => ph::SIDEBAR,
        "Язык (Language)" => ph::TRANSLATE,
        "История" => ph::CLOCK_COUNTER_CLOCKWISE,
        "Опорная картинка…" => ph::IMAGE,
        "Масштаб интерфейса" => ph::TEXT_AA,
        "Быстрые клавиши…" | "Клавиши…" => ph::KEYBOARD,
        "Размер образцов" => ph::SWATCHES,
        "Полный экран" => ph::CORNERS_OUT,
        // Помощь
        "Настройки…" => ph::GEAR,
        "GrazySAI на GitHub" => ph::GITHUB_LOGO,
        "Проверить обновления…" => ph::DOWNLOAD_SIMPLE,
        "О программе" => ph::INFO,
        // Сетка инструментов, группы, библиотека
        "Сохранить кисть…" => ph::FLOPPY_DISK,
        "Новая группа" => ph::FOLDER_PLUS,
        "Переименовать…" => ph::PENCIL_SIMPLE,
        "Группа как в SAI 2" => ph::STACK_PLUS,
        "Запомнить настройки" => ph::BOOKMARK_SIMPLE,
        "Вернуть запомненные" => ph::ARROW_U_UP_LEFT,
        "Отменить возврат" => ph::ARROW_U_UP_RIGHT,
        "Добавить…" => ph::PLUS,
        "В группу" => ph::FOLDER_SIMPLE,
        "Запись процесса" => ph::RECORD,
        "Начать запись" => ph::RECORD,
        "Остановить запись" => ph::STOP,
        "Выгрузить видео…" => ph::FILM_STRIP,
        "В закладки" => ph::STAR,
        "Выше" => ph::ARROW_UP,
        "Ниже" => ph::ARROW_DOWN,
        "Убрать из закладок" => ph::STAR_HALF,
        "Открыть" => ph::FOLDER_OPEN,
        "Сохранить сюда" => ph::FLOPPY_DISK,
        "Удалить…" => ph::TRASH,
        _ => return None,
    })
}

/// Подпись пункта меню: знак (или пустое место той же ширины) и перевод ключа.
pub fn label(key: &'static str) -> LayoutJob {
    with_icon(icon(key), tr(key))
}

/// Подпись со знаком `icon` и готовым текстом.
pub fn with_icon(icon: Option<&str>, text: &str) -> LayoutJob {
    let font = FONT.lock().unwrap().clone().unwrap_or_else(|| FontId::proportional(14.0));
    let mut job = LayoutJob::default();
    // пустое место — тот же знак, только прозрачный: ширина совпадает
    let (sign, color) = match icon {
        Some(i) => (i, Color32::PLACEHOLDER),
        None => (ph::SQUARE, Color32::TRANSPARENT),
    };
    job.append(sign, 0.0, TextFormat { font_id: font.clone(), color, ..Default::default() });
    job.append(text, font.size * 0.45, TextFormat { font_id: font, color: Color32::PLACEHOLDER, ..Default::default() });
    job
}

/// Кнопка пункта меню со знаком.
pub fn button(key: &'static str) -> egui::Button<'static> {
    egui::Button::new(label(key))
}
