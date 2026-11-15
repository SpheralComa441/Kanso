//! UI language and the user-facing texts produced by the backend.
//!
//! The language is a process-wide setting (one user, one window): the desktop shell sets it at
//! startup and whenever the settings change, and every message built afterwards uses it.

use std::sync::atomic::{AtomicU8, Ordering};

/// Languages the interface is available in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    #[default]
    It,
    En,
    Es,
    Ru,
}

impl Lang {
    pub const ALL: [Lang; 4] = [Lang::It, Lang::En, Lang::Es, Lang::Ru];

    /// Two-letter code stored in `config.json` and in each game's `metadata_lang`.
    pub fn code(self) -> &'static str {
        match self {
            Lang::It => "it",
            Lang::En => "en",
            Lang::Es => "es",
            Lang::Ru => "ru",
        }
    }

    /// Parses a language code or locale tag (`it`, `en-US`, `ru_RU`); `None` if unsupported.
    pub fn from_code(raw: &str) -> Option<Self> {
        let primary = raw
            .trim()
            .split(['-', '_'])
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        Lang::ALL.into_iter().find(|lang| lang.code() == primary)
    }

    /// Value of the Steam Store API `l=` parameter.
    pub fn steam_language(self) -> &'static str {
        match self {
            Lang::It => "italian",
            Lang::En => "english",
            Lang::Es => "spanish",
            Lang::Ru => "russian",
        }
    }

    /// Locale understood by the Epic Games Store.
    pub fn epic_locale(self) -> &'static str {
        match self {
            Lang::It => "it",
            Lang::En => "en-US",
            Lang::Es => "es-ES",
            Lang::Ru => "ru",
        }
    }

    /// Closest locale GOG's catalog supports (it has no Italian or Spanish).
    pub fn gog_locale(self) -> &'static str {
        match self {
            Lang::Ru => "ru-RU",
            Lang::It | Lang::En | Lang::Es => "en-US",
        }
    }

    fn index(self) -> usize {
        match self {
            Lang::It => 0,
            Lang::En => 1,
            Lang::Es => 2,
            Lang::Ru => 3,
        }
    }
}

static CURRENT_LANGUAGE: AtomicU8 = AtomicU8::new(0);

/// Sets the language of every backend message produced from now on.
pub fn set_language(lang: Lang) {
    CURRENT_LANGUAGE.store(lang.index() as u8, Ordering::Relaxed);
}

pub fn current_language() -> Lang {
    Lang::ALL[usize::from(CURRENT_LANGUAGE.load(Ordering::Relaxed)).min(Lang::ALL.len() - 1)]
}

/// Short user-facing texts. Details that follow them (paths, OS errors) are appended as-is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum T {
    CannotRead,
    CannotSave,
    CannotCreateFolder,
    InvalidFormat,
    CannotOpenLibraryDb,
    SessionNotSaved,
    LegacyImportFailed,
    LibraryDbError,
    LibraryDbNewer,
    LibraryDbSetAside,
    LibraryDbRecreated,
    SgdbKeyMissing,
    SgdbKeyInvalid,
    SgdbHttpError,
    SgdbNetworkError,
    SgdbBadResponse,
    CoverDownloadFailed,
    CoverSaveFailed,
    GameNotFound,
    ImageFileNotFound,
    InvalidManifest,
    CustomFolderMissing,
    NotAFolder,
    LocalGame,
    AppDataUnavailable,
    ScanFailed,
    SyncFailed,
    LaunchFailed,
    NoLaunchTarget,
    ExeNotFound,
    ThemeInvalid,
    ThemeNotFound,
    GameNotInstalled,
    NoInstallTarget,
    PluginInvalid,
    PluginNotFound,
    PluginFailed,
    UrlNotAllowed,
    OperationFailed,
    StoreNotSupported,
    SignInCodeMissing,
    SignInRefused,
    StoreUnreachable,
    SignInFailed,
    SignInOn,
    SignInWindowBusy,
    SignInTimedOut,
    LibraryNotListed,
    TrayOpen,
    TrayQuit,
}

impl T {
    /// The text in every language, in [`Lang::ALL`] order.
    pub fn all(self) -> [&'static str; 4] {
        match self {
            T::CannotRead => ["Impossibile leggere", "Cannot read", "No se puede leer", "Не удалось прочитать"],
            T::CannotSave => ["Impossibile salvare", "Cannot save", "No se puede guardar", "Не удалось сохранить"],
            T::CannotCreateFolder => [
                "Impossibile creare la cartella",
                "Cannot create the folder",
                "No se puede crear la carpeta",
                "Не удалось создать папку",
            ],
            T::InvalidFormat => ["Formato non valido in", "Invalid format in", "Formato no válido en", "Неверный формат в"],
            T::CannotOpenLibraryDb => [
                "Impossibile aprire il database della libreria",
                "Cannot open the library database",
                "No se puede abrir la base de datos de la biblioteca",
                "Не удалось открыть базу данных библиотеки",
            ],
            T::SessionNotSaved => [
                "Le modifiche di questa sessione non verranno salvate.",
                "Changes made in this session will not be saved.",
                "Los cambios de esta sesión no se guardarán.",
                "Изменения этого сеанса не будут сохранены.",
            ],
            T::LegacyImportFailed => [
                "Importazione della libreria precedente non riuscita",
                "Could not import the previous library",
                "No se pudo importar la biblioteca anterior",
                "Не удалось импортировать прежнюю библиотеку",
            ],
            T::LibraryDbError => [
                "Errore del database della libreria",
                "Library database error",
                "Error de la base de datos de la biblioteca",
                "Ошибка базы данных библиотеки",
            ],
            T::LibraryDbNewer => [
                "Il database della libreria è stato creato da una versione più recente di Kanso.",
                "The library database was created by a newer version of Kanso.",
                "La base de datos de la biblioteca fue creada por una versión más reciente de Kanso.",
                "База данных библиотеки создана более новой версией Kanso.",
            ],
            T::LibraryDbSetAside => [
                "Il database della libreria era illeggibile ed è stato messo da parte in",
                "The library database was unreadable and has been set aside in",
                "La base de datos de la biblioteca era ilegible y se ha apartado en",
                "База данных библиотеки не читалась и была отложена в",
            ],
            T::LibraryDbRecreated => [
                "Ne è stato creato uno nuovo: preferiti e modifiche manuali vanno reimpostati.",
                "A new one was created: favorites and manual edits have to be set again.",
                "Se ha creado una nueva: hay que volver a definir favoritos y cambios manuales.",
                "Создана новая: избранное и ручные правки нужно задать заново.",
            ],
            T::SgdbKeyMissing => [
                "Configura prima la SteamGridDB API Key nelle Impostazioni.",
                "Set your SteamGridDB API key in Settings first.",
                "Configura primero la clave API de SteamGridDB en Ajustes.",
                "Сначала укажите API-ключ SteamGridDB в настройках.",
            ],
            T::SgdbKeyInvalid => [
                "SteamGridDB API Key non valida o non autorizzata. Verifica la chiave nelle Impostazioni.",
                "The SteamGridDB API key is invalid or not authorized. Check it in Settings.",
                "La clave API de SteamGridDB no es válida o no está autorizada. Revísala en Ajustes.",
                "API-ключ SteamGridDB недействителен или не авторизован. Проверьте его в настройках.",
            ],
            T::SgdbHttpError => ["Errore SteamGridDB HTTP", "SteamGridDB HTTP error", "Error HTTP de SteamGridDB", "Ошибка HTTP SteamGridDB"],
            T::SgdbNetworkError => [
                "Errore di rete SteamGridDB",
                "SteamGridDB network error",
                "Error de red de SteamGridDB",
                "Сетевая ошибка SteamGridDB",
            ],
            T::SgdbBadResponse => [
                "Risposta SteamGridDB non valida",
                "Invalid SteamGridDB response",
                "Respuesta de SteamGridDB no válida",
                "Некорректный ответ SteamGridDB",
            ],
            T::CoverDownloadFailed => [
                "Impossibile scaricare l'immagine di copertina",
                "Could not download the cover image",
                "No se pudo descargar la carátula",
                "Не удалось загрузить обложку",
            ],
            T::CoverSaveFailed => [
                "Impossibile salvare la copertina",
                "Could not save the cover",
                "No se pudo guardar la carátula",
                "Не удалось сохранить обложку",
            ],
            T::GameNotFound => [
                "Gioco non trovato nella libreria",
                "Game not found in the library",
                "Juego no encontrado en la biblioteca",
                "Игра не найдена в библиотеке",
            ],
            T::ImageFileNotFound => [
                "File immagine non trovato",
                "Image file not found",
                "Archivo de imagen no encontrado",
                "Файл изображения не найден",
            ],
            T::InvalidManifest => ["Manifest non valido", "Invalid manifest", "Manifiesto no válido", "Некорректный манифест"],
            T::CustomFolderMissing => [
                "La cartella personalizzata non esiste o non è raggiungibile",
                "The custom folder does not exist or cannot be reached",
                "La carpeta personalizada no existe o no es accesible",
                "Пользовательская папка не существует или недоступна",
            ],
            T::NotAFolder => [
                "Il percorso non è una cartella valida",
                "The path is not a valid folder",
                "La ruta no es una carpeta válida",
                "Путь не является папкой",
            ],
            T::LocalGame => ["Gioco locale", "Local game", "Juego local", "Локальная игра"],
            T::AppDataUnavailable => [
                "Impossibile risolvere la cartella dati dell'app",
                "Cannot resolve the app data folder",
                "No se puede resolver la carpeta de datos de la aplicación",
                "Не удалось определить папку данных приложения",
            ],
            T::ScanFailed => [
                "Errore durante la scansione della libreria",
                "Library scan failed",
                "Error al escanear la biblioteca",
                "Ошибка сканирования библиотеки",
            ],
            T::SyncFailed => [
                "Errore di sincronizzazione dei metadata",
                "Metadata sync failed",
                "Error al sincronizar los metadatos",
                "Ошибка синхронизации метаданных",
            ],
            T::LaunchFailed => ["Impossibile avviare", "Could not launch", "No se pudo iniciar", "Не удалось запустить"],
            T::NoLaunchTarget => [
                "Nessun eseguibile o URI di avvio configurato per",
                "No executable or launch URI configured for",
                "No hay ejecutable ni URI de inicio configurado para",
                "Не задан исполняемый файл или URI запуска для",
            ],
            T::ExeNotFound => [
                "Eseguibile non trovato sul disco",
                "Executable not found on disk",
                "Ejecutable no encontrado en el disco",
                "Исполняемый файл не найден на диске",
            ],
            T::ThemeInvalid => ["Tema non valido", "Invalid theme", "Tema no válido", "Некорректная тема"],
            T::ThemeNotFound => ["Tema non trovato", "Theme not found", "Tema no encontrado", "Тема не найдена"],
            T::GameNotInstalled => ["Il gioco non è installato", "The game is not installed", "El juego no está instalado", "Игра не установлена"],
            T::NoInstallTarget => ["Kanso non può avviare l'installazione per questo store: usa il suo client", "Kanso cannot start the installation for this store: use its client", "Kanso no puede iniciar la instalación para esta tienda: usa su cliente", "Kanso не может запустить установку для этого магазина: используйте его клиент"],
            T::PluginInvalid => ["Plugin non valido", "Invalid plugin", "Plugin no válido", "Недопустимый плагин"],
            T::PluginNotFound => ["Plugin non trovato", "Plugin not found", "Plugin no encontrado", "Плагин не найден"],
            T::PluginFailed => ["Il plugin non ha risposto correttamente", "The plugin did not answer correctly", "El plugin no respondió correctamente", "Плагин не ответил корректно"],
            T::UrlNotAllowed => ["Indirizzo non consentito", "URL not allowed", "Dirección no permitida", "Адрес не разрешён"],
            T::OperationFailed => ["Operazione non riuscita", "Operation failed", "La operación ha fallado", "Операция не выполнена"],
            T::StoreNotSupported => ["Accesso non disponibile per questo store", "Sign-in is not available for this store", "El inicio de sesión no está disponible para esta tienda", "Вход для этого магазина недоступен"],
            T::SignInCodeMissing => [
                "Non trovo un codice in quello che hai incollato. Accedi sulla pagina dello store e copia il codice che compare alla fine",
                "There is no code in what you pasted. Sign in on the store's page and copy the code shown at the end",
                "No encuentro un código en lo que has pegado. Inicia sesión en la página de la tienda y copia el código que aparece al final",
                "В вставленном тексте нет кода. Войдите на странице магазина и скопируйте код, который появится в конце",
            ],
            T::SignInRefused => [
                "Lo store non ha accettato il codice: è scaduto o è già stato usato. Riapri la pagina di accesso e copiane uno nuovo",
                "The store did not accept the code: it expired or was already used. Open the sign-in page again and copy a new one",
                "La tienda no ha aceptado el código: ha caducado o ya se ha usado. Vuelve a abrir la página de inicio de sesión y copia uno nuevo",
                "Магазин не принял код: он истёк или уже использован. Откройте страницу входа ещё раз и скопируйте новый",
            ],
            T::StoreUnreachable => ["Impossibile raggiungere lo store. Controlla la connessione e riprova", "The store could not be reached. Check the connection and try again", "No se puede conectar con la tienda. Comprueba la conexión e inténtalo de nuevo", "Не удалось связаться с магазином. Проверьте соединение и повторите попытку"],
            T::SignInFailed => ["Accesso non riuscito", "Sign-in failed", "No se ha podido iniciar sesión", "Не удалось войти"],
            T::SignInOn => ["Accesso a {store}", "Sign in to {store}", "Iniciar sesión en {store}", "Вход в {store}"],
            T::SignInWindowBusy => [
                "La finestra di accesso di questo store è già aperta, oppure Kanso sta leggendo la sua libreria. Riprova tra un momento",
                "The sign-in window of this store is already open, or Kanso is reading its library. Try again in a moment",
                "La ventana de inicio de sesión de esta tienda ya está abierta, o Kanso está leyendo su biblioteca. Inténtalo de nuevo en un momento",
                "Окно входа этого магазина уже открыто, или Kanso читает его библиотеку. Повторите попытку чуть позже",
            ],
            T::SignInTimedOut => [
                "La finestra di accesso è rimasta aperta troppo a lungo senza che lo store confermasse l'accesso",
                "The sign-in window stayed open too long without the store confirming the sign-in",
                "La ventana de inicio de sesión ha estado abierta demasiado tiempo sin que la tienda confirmara el inicio de sesión",
                "Окно входа было открыто слишком долго, а магазин так и не подтвердил вход",
            ],
            T::LibraryNotListed => [
                "Accesso riuscito, ma lo store non ha dato l'elenco dei giochi",
                "Signed in, but the store did not list the games",
                "Sesión iniciada, pero la tienda no ha dado la lista de juegos",
                "Вход выполнен, но магазин не выдал список игр",
            ],
            T::TrayOpen => ["Apri Kanso", "Open Kanso", "Abrir Kanso", "Открыть Kanso"],
            T::TrayQuit => ["Esci", "Quit", "Salir", "Выход"],
        }
    }

    pub fn in_lang(self, lang: Lang) -> &'static str {
        self.all()[lang.index()]
    }

    /// The text in the current UI language.
    pub fn s(self) -> &'static str {
        self.in_lang(current_language())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_language_codes_and_locale_tags() {
        assert_eq!(Lang::from_code("it"), Some(Lang::It));
        assert_eq!(Lang::from_code("en-US"), Some(Lang::En));
        assert_eq!(Lang::from_code(" RU_ru "), Some(Lang::Ru));
        assert_eq!(Lang::from_code("es-419"), Some(Lang::Es));
        assert_eq!(Lang::from_code("de"), None);
        assert_eq!(Lang::from_code(""), None);
        for lang in Lang::ALL {
            assert_eq!(Lang::from_code(lang.code()), Some(lang));
        }
    }

    #[test]
    fn every_text_is_translated_in_every_language() {
        // Exhaustive by construction: `all()` is a match over every variant. Spot-check content.
        assert_eq!(T::TrayQuit.in_lang(Lang::It), "Esci");
        assert_eq!(T::TrayQuit.in_lang(Lang::En), "Quit");
        assert_eq!(T::TrayQuit.in_lang(Lang::Es), "Salir");
        assert_eq!(T::TrayQuit.in_lang(Lang::Ru), "Выход");
        for text in [T::GameNotFound, T::SgdbKeyInvalid, T::LaunchFailed, T::LibraryDbSetAside] {
            let all = text.all();
            assert!(all.iter().all(|s| !s.trim().is_empty()));
            // No language silently reuses another one's text
            for i in 0..all.len() {
                for j in (i + 1)..all.len() {
                    assert_ne!(all[i], all[j], "{text:?}");
                }
            }
        }
    }
}
