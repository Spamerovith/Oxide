//! Язык интерфейса: русский (исходные строки в коде) и английский (таблица ниже)
//!
//! Строки интерфейса пишутся по-русски и оборачиваются в `tr("...")` или `trf("... {}", &[&x])`.
//! Для английского каждая такая строка должна быть в `EN`, тест `all_strings_translated` это проверяет.
//! Для новых строк, которых нет в таблице, удобнее `l("рус", "eng")`

use std::collections::HashMap;
use std::fmt::Display;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    Ru,
    En,
}

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::Ru, Lang::En];

    /// Название языка на нём самом
    pub fn native_name(self) -> &'static str {
        match self {
            Lang::Ru => "Русский",
            Lang::En => "English",
        }
    }

    /// Язык интерфейса Windows: русский, если Windows русская, иначе английский
    pub fn system() -> Lang {
        let id = unsafe { windows::Win32::Globalization::GetUserDefaultUILanguage() };
        if id & 0x3FF == 0x19 {
            Lang::Ru
        } else {
            Lang::En
        }
    }
}

static LANG: AtomicU8 = AtomicU8::new(0);

pub fn set(lang: Lang) {
    LANG.store(lang as u8, Ordering::Relaxed);
}

pub fn current() -> Lang {
    if LANG.load(Ordering::Relaxed) == Lang::En as u8 {
        Lang::En
    } else {
        Lang::Ru
    }
}

pub fn is_en() -> bool {
    current() == Lang::En
}

/// Перевод строки интерфейса
pub fn tr(s: &'static str) -> &'static str {
    if !is_en() {
        return s;
    }
    static MAP: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    MAP.get_or_init(|| EN.iter().copied().collect()).get(s).copied().unwrap_or(s)
}

/// Перевод строки с подстановкой: каждое `{}` по порядку заменяется на очередной аргумент
pub fn trf(s: &'static str, args: &[&dyn Display]) -> String {
    let t = tr(s);
    let mut out = String::with_capacity(t.len() + 16);
    let mut rest = t;
    let mut i = 0;
    while let Some(p) = rest.find("{}") {
        out.push_str(&rest[..p]);
        if let Some(a) = args.get(i) {
            out.push_str(&a.to_string());
        }
        i += 1;
        rest = &rest[p + 2..];
    }
    out.push_str(rest);
    out
}

/// Строка, у которой оба варианта написаны прямо на месте
pub fn l(ru: &'static str, en: &'static str) -> &'static str {
    if is_en() {
        en
    } else {
        ru
    }
}

/// «Элементов: 5» / «5 items»
pub fn items(n: usize) -> String {
    if is_en() {
        if n == 1 {
            "1 item".into()
        } else {
            format!("{n} items")
        }
    } else {
        format!("Элементов: {n}")
    }
}

/// «Выбрано 3 элемента» / «3 items selected»
pub fn items_selected(n: usize) -> String {
    if is_en() {
        if n == 1 {
            "1 item selected".into()
        } else {
            format!("{n} items selected")
        }
    } else {
        use crate::util::plural;
        format!("{} {n} {}", plural(n, "Выбран", "Выбрано", "Выбрано"), plural(n, "элемент", "элемента", "элементов"))
    }
}

/// Английские названия - как в английском Проводнике Windows 11
const EN: &[(&str, &str)] = &[
    ("      Индексация для быстрого поиска: просмотрено папок {}", "      Indexing for fast search: {} folders scanned"),
    ("   |   Поиск продолжается…", "   |   Still searching…"),
    (" Проверка изменений: папок {} из {}", " Checking for changes: {} of {} folders"),
    (" Построение… просмотрено папок: {}", " Building… folders scanned: {}"),
    (
        "Oxide - независимый проект с открытым исходным кодом. Он не связан с Microsoft, не одобрен и не поддерживается ею. Windows и Windows 11 - товарные знаки Microsoft Corporation. Сторонние библиотеки и их лицензии перечислены в файле THIRD_PARTY_LICENSES.md",
        "Oxide is an independent open source project. It is not affiliated with, endorsed or supported by Microsoft. Windows and Windows 11 are trademarks of Microsoft Corporation. Third-party libraries and their licenses are listed in THIRD_PARTY_LICENSES.md",
    ),
    ("Windows 11 - светлая", "Windows 11 - Light"),
    ("Windows 11 - тёмная", "Windows 11 - Dark"),
    ("\nДата изменения: {}", "\nDate modified: {}"),
    ("{} ({} байт)", "{} ({} bytes)"),
    ("{} Б", "{} bytes"),
    ("{} свободно из {}", "{} free of {}"),
    ("Архив", "Archive"),
    ("Атрибуты", "Attributes"),
    ("Больше не показывать", "Don't show again"),
    ("Бумага", "Paper"),
    ("Быстрый доступ", "Quick access"),
    (
        "Быстрый проводник для Windows на Rust. Интерфейс повторяет Проводник Windows 11, а файловые операции выполняет сам движок Windows",
        "A fast file explorer for Windows written in Rust. The interface follows Windows 11 File Explorer, and file operations are done by Windows itself",
    ),
    ("В буфере обмена нет файлов", "There are no files on the clipboard"),
    (
        "В индексе {} объектов (на диске {}, обновлён {}). Индекс читается с диска по мере надобности и почти не занимает оперативную память",
        "The index holds {} items ({} on disk, updated {}). It is read from disk as needed and uses almost no memory",
    ),
    ("Вверх к «{}» (Alt+Стрелка вверх)", "Up to \"{}\" (Alt + Up Arrow)"),
    ("Версия {}", "Version {}"),
    ("Вид", "View"),
    ("Восстанавливать вкладки при следующем запуске", "Restore tabs on next launch"),
    ("Вперёд к «{}» (Alt+Стрелка вправо)", "Forward to {} (Alt + Right Arrow)"),
    ("Вставить (Ctrl+V)", "Paste (Ctrl+V)"),
    ("Вставить", "Paste"),
    ("Выберите один файл, чтобы получить сведения о нём", "Select a single file to get more information"),
    ("Выберите файл для предварительного просмотра", "Select a file to preview"),
    ("Выделить все", "Select all"),
    ("Вырезать (Ctrl+X)", "Cut (Ctrl+X)"),
    ("Галерея", "Gallery"),
    ("Главная", "Home"),
    ("Графит", "Graphite"),
    ("Дата изменения", "Date modified"),
    ("Дата изменения: {}", "Date modified: {}"),
    ("Дата открытия", "Date accessed"),
    ("Дата создания", "Date created"),
    ("Дата удаления", "Date Deleted"),
    ("Дата", "Date"),
    ("Дублировать вкладку", "Duplicate tab"),
    ("Загрузка…", "Working on it…"),
    ("Закрепить на панели быстрого доступа", "Pin to Quick access"),
    ("Закрыть вкладки справа", "Close tabs to the right"),
    ("Закрыть вкладку (Ctrl+W)", "Close tab (Ctrl+W)"),
    ("Закрыть вкладку", "Close tab"),
    ("Закрыть другие вкладки", "Close other tabs"),
    ("Закрыть", "Close"),
    ("Запуск от имени администратора", "Run as administrator"),
    ("Здесь появятся файлы, которые вы недавно открывали", "After you've opened some files, we'll show the most recent ones here"),
    ("Извлечь все...", "Extract All..."),
    ("Имя файла не должно содержать следующих знаков: \\ / : * ? \" < > |", "A file name can't contain any of the following characters: \\ / : * ? \" < > |"),
    ("Имя", "Name"),
    ("Индекс ещё не построен: поиск идёт по папкам напрямую", "The index isn't built yet: search scans folders directly"),
    ("Индексировать диски для мгновенного поиска", "Index drives for instant search"),
    ("Интерфейс", "Appearance"),
    ("Использовать флажки для выбора элементов", "Use check boxes to select items"),
    ("Исходное расположение", "Original Location"),
    ("Как в Windows", "Same as Windows"),
    ("Компактный вид", "Compact view"),
    ("Конфиденциальность", "Privacy"),
    ("Копировать (Ctrl+C)", "Copy (Ctrl+C)"),
    ("Копировать", "Copy here"),
    ("Переместить", "Move here"),
    ("Создать ярлыки", "Create shortcuts here"),
    ("Отмена", "Cancel"),
    ("Копировать как путь", "Copy as path"),
    ("Корзина", "Recycle Bin"),
    ("Крупные значки", "Large icons"),
    ("Локальный диск ({}:)", "Local Disk ({}:)"),
    ("Мелкие значки", "Small icons"),
    ("Назад к «{}» (Alt+Стрелка влево)", "Back to {} (Alt + Left Arrow)"),
    ("Не удалось выполнить операцию", "The operation couldn't be completed"),
    ("Не удалось извлечь: {}", "Couldn't extract: {}"),
    ("Не удалось создать ZIP: {}", "Couldn't create the ZIP file: {}"),
    ("Не удалось создать папку", "Couldn't create the folder"),
    ("Не удалось создать файл", "Couldn't create the file"),
    ("Не удаётся найти «{}». Проверьте правильность написания и повторите попытку", "Oxide can't find \"{}\". Check the spelling and try again"),
    ("Не удаётся открыть «{}»", "Can't open \"{}\""),
    ("Нет элементов, удовлетворяющих условиям поиска", "No items match your search"),
    ("Нечего отменять", "Nothing to undo"),
    ("Новая вкладка (Ctrl+T)", "New tab (Ctrl+T)"),
    ("Новая вкладка", "New tab"),
    ("Новая папка", "New folder"),
    ("О программе Oxide", "About Oxide"),
    ("ОК", "OK"),
    ("Область просмотра", "Preview pane"),
    ("Область сведений (Alt+Shift+P)", "Details pane (Alt+Shift+P)"),
    ("Область сведений", "Details pane"),
    ("Обновить «{}» (F5)", "Refresh \"{}\" (F5)"),
    ("Обновить", "Refresh"),
    ("Обратить выделение", "Invert selection"),
    ("Общий доступ для этих элементов недоступен", "These items can't be shared"),
    ("Общий размер файлов: {}", "Total file size: {}"),
    ("Обычные значки", "Medium icons"),
    ("Огромные значки", "Extra large icons"),
    ("Отказано в доступе", "Access is denied"),
    ("Открепить от панели быстрого доступа", "Unpin from Quick access"),
    ("Открывать проводник для", "Open Oxide to"),
    ("Открыть в Терминале", "Open in Terminal"),
    ("Открыть в новой вкладке", "Open in new tab"),
    ("Открыть в новом окне", "Open in new window"),
    ("Открыть расположение файла", "Open file location"),
    ("Открыть с помощью", "Open with"),
    ("Открыть", "Open"),
    ("Отменить", "Undo"),
    ("Отображает сведения о каждом элементе в окне (Ctrl+Shift+6)", "Displays information about each item in the window (Ctrl+Shift+6)"),
    ("Отображение элементов в виде крупных эскизов (Ctrl+Shift+2)", "Display items by using large thumbnails (Ctrl+Shift+2)"),
    ("Очистить корзину", "Empty Recycle Bin"),
    ("Очистить", "Clear"),
    ("Ошибка {}", "Error {}"),
    ("Папка с файлами", "File folder"),
    ("Папка", "Folder"),
    ("Папку", "Folder"),
    ("Параметры", "Options"),
    ("Переименовать (F2)", "Rename (F2)"),
    ("Переместить в новое окно", "Move to new window"),
    ("Перестроить индекс", "Rebuild index"),
    ("Плитка", "Tiles"),
    ("По возрастанию", "Ascending"),
    ("По убыванию", "Descending"),
    ("Повторить", "Try again"),
    ("Поделиться", "Share"),
    ("Подробнее", "See more"),
    ("Поиск в: {}", "Search {}"),
    ("Поиск", "Search"),
    ("Поиск: {}", "Search: {}"),
    ("Поиск…", "Searching…"),
    ("Показать дополнительные параметры", "Show more options"),
    ("Показать", "Show"),
    ("Показывать защищённые системные файлы", "Show protected operating system files"),
    ("Показывать недавно использовавшиеся файлы на главной", "Show recently used files in Home"),
    ("Показывать расширения имён файлов", "Show file name extensions"),
    ("Показывать скрытые файлы, папки и диски", "Show hidden files, folders and drives"),
    ("Последние", "Recent"),
    ("Проводник Oxide", "Oxide Explorer"),
    ("Просмотреть", "View"),
    ("Размер", "Size"),
    ("Размер: {}", "Size: {}"),
    ("Расположение недоступно", "Location is not available"),
    ("Расположение недоступно: путь не найден", "Location is not available: the path was not found"),
    ("Расположение", "Location"),
    ("Расширения имён файлов", "File name extensions"),
    ("Результаты поиска в «{}»", "Search Results in {}"),
    ("Сведения", "Details"),
    ("Свойства", "Properties"),
    ("Сделать фоновым изображением рабочего стола", "Set as desktop background"),
    ("Северная ночь", "Nordic Night"),
    ("Сетевые расположения", "Network locations"),
    ("Сеть", "Network"),
    ("Сжать в ZIP-файл", "Compress to ZIP file"),
    ("Скрытые элементы", "Hidden items"),
    ("Снять выделение", "Select none"),
    ("Содержимое", "Content"),
    ("Создать", "New"),
    ("Сортировать", "Sort"),
    ("Сортировка", "Sort by"),
    ("Список", "List"),
    ("Таблица", "Details"),
    ("Текстовый документ", "New Text Document"),
    ("Тип элемента", "Item type"),
    ("Тип", "Type"),
    ("Тип: {}\nРазмер: {}", "Type: {}\nSize: {}"),
    ("Удалить (Delete)", "Delete (Delete)"),
    ("Устройства и диски", "Devices and drives"),
    ("Файл \"{}\"", "{} File"),
    ("Файл", "File"),
    ("Флажки элементов", "Item check boxes"),
    ("Эта папка пуста", "This folder is empty"),
    ("Этот компьютер", "This PC"),
    ("системный", "system"),
    ("скрытый", "hidden"),
    ("только чтение", "read-only"),
    // Корзина
    ("Удаление папки", "Delete Folder"),
    ("Удаление файла", "Delete File"),
    ("Удаление нескольких элементов", "Delete Multiple Items"),
    ("Не удалось восстановить из корзины", "Couldn't restore from the Recycle Bin"),
    ("Корзина пуста", "The Recycle Bin is empty"),
    ("Восстановить", "Restore"),
    ("Восстановить все объекты", "Restore all items"),
    ("Восстановить выделенные объекты", "Restore the selected items"),
    ("Вы действительно хотите безвозвратно удалить папку «{}»?", "Are you sure you want to permanently delete the folder \"{}\"?"),
    ("Вы действительно хотите безвозвратно удалить файл «{}»?", "Are you sure you want to permanently delete the file \"{}\"?"),
    ("Вы действительно хотите безвозвратно удалить эти элементы ({})?", "Are you sure you want to permanently delete these {} items?"),
    ("Вы действительно хотите безвозвратно удалить все элементы из корзины ({})?", "Are you sure you want to permanently delete all {} items in the Recycle Bin?"),
    ("Да", "Yes"),
    ("Нет", "No"),
    // Свойства
    ("Свойства: {}", "{} Properties"),
    ("Общие", "General"),
    ("Доступ", "Sharing"),
    ("Безопасность", "Security"),
    ("Предыдущие версии", "Previous Versions"),
    ("Настройка", "Customize"),
    ("Ярлык", "Shortcut"),
    ("Совместимость", "Compatibility"),
    ("Подробно", "Details"),
    ("Тип:", "Type:"),
    ("Тип файла:", "Type of file:"),
    ("Приложение:", "Opens with:"),
    ("Изменить...", "Change..."),
    ("Несколько типов", "Multiple types"),
    ("Все типа {}", "All of type {}"),
    ("Расположение:", "Location:"),
    ("Все в {}", "All in {}"),
    ("Различные папки", "Various folders"),
    ("Размер:", "Size:"),
    ("На диске:", "Size on disk:"),
    ("Содержит:", "Contains:"),
    ("Файлов: {}; папок: {}", "{} Files, {} Folders"),
    ("Файлов: {}, папок: {}", "{} files, {} folders"),
    ("Создан:", "Created:"),
    ("Изменён:", "Modified:"),
    ("Открыт:", "Accessed:"),
    ("Атрибуты:", "Attributes:"),
    ("Только для чтения\n(применимо только к файлам в папке)", "Read-only\n(Only applies to files in folder)"),
    ("Только для чтения", "Read-only"),
    ("Скрытый", "Hidden"),
    ("Другие...", "Advanced..."),
    ("Только к этой папке", "Apply changes to this folder only"),
    ("К этой папке и ко всем вложенным папкам и файлам", "Apply changes to this folder, subfolders and files"),
    ("Применить", "Apply"),
    ("Не удалось изменить атрибуты у элементов: {}", "Couldn't change attributes of {} items"),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Каждая строка из `tr("...")` и `trf("...")` в исходниках должна иметь английский перевод
    #[test]
    fn all_strings_translated() {
        let keys: std::collections::HashSet<&str> = EN.iter().map(|p| p.0).collect();
        let mut missing = vec![];
        let mut stack = vec![std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/src"))];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                if p.extension().and_then(|x| x.to_str()) != Some("rs") || p.ends_with("i18n.rs") {
                    continue;
                }
                let src = std::fs::read_to_string(&p).unwrap();
                for marker in ["tr(\"", "trf(\""] {
                    let mut rest = src.as_str();
                    while let Some(i) = rest.find(marker) {
                        let before = &rest[..i];
                        rest = &rest[i + marker.len()..];
                        if before.ends_with(|c: char| c.is_alphanumeric() || c == '_') {
                            continue;
                        }
                        // Конец литерала: первая кавычка без обратной косой черты перед ней
                        let b = rest.as_bytes();
                        let mut j = 0;
                        while j < b.len() && !(b[j] == b'"' && (j == 0 || b[j - 1] != b'\\' || (j >= 2 && b[j - 2] == b'\\'))) {
                            j += 1;
                        }
                        let raw = &rest[..j];
                        let lit = raw.replace("\\n", "\n").replace("\\\"", "\"").replace("\\\\", "\\");
                        if !keys.contains(lit.as_str()) {
                            missing.push(format!("{}: {}", p.display(), raw));
                        }
                    }
                }
            }
        }
        assert!(missing.is_empty(), "Нет перевода:\n{}", missing.join("\n"));
    }

    #[test]
    fn substitution() {
        set(Lang::En);
        assert_eq!(trf("{} свободно из {}", &[&"1 GB", &"2 GB"]), "1 GB free of 2 GB");
        assert_eq!(items(1), "1 item");
        set(Lang::Ru);
        assert_eq!(trf("{} свободно из {}", &[&"1 ГБ", &"2 ГБ"]), "1 ГБ свободно из 2 ГБ");
        assert_eq!(items_selected(3), "Выбрано 3 элемента");
    }
}
