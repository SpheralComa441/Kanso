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
    FolderNotFound,
    /// Follows "RetroArch: <n> ".
    RetroArchNoCore,
    RetroArchCoreMissing,
    EmulatorNotFound,
    ThemeInvalid,
    ThemeNotFound,
    GameNotInstalled,
    NoInstallTarget,
    NoUninstallTarget,
    DlssNotNvidia,
    DlssNothingOlder,
    /// Followed by ": <what the system said>".
    DlssBusy,
    DlssUpdatesOff,
    /// Followed by ": <what went wrong>".
    DlssDownloadFailed,
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
    SignInPortBusy,
    BrowserSignInTimedOut,
    BrowserSignInDone,
    BrowserSignInRefused,
    LibraryNotListed,
    ClientGames,
    AccountGames,
    AccountSubscriptionGames,
    AccountGamesLeftOut,
    TrayOpen,
    TrayQuit,
    UpdateCheckFailed,
    UpdateRateLimited,
    UpdateDownloadFailed,
    UpdateChecksumMismatch,
    UpdateNoChecksum,
    UpdateNoInstaller,
    UpdateInstallFailed,
    UpdateCancelled,
    UpdateInProgress,
    UpdateVersionMismatch,
    UpdateWindowsOnly,
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
            T::FolderNotFound => [
                "La cartella del gioco non è più sul disco",
                "The game's folder is no longer on disk",
                "La carpeta del juego ya no está en el disco",
                "Папка игры больше не найдена на диске",
            ],
            T::RetroArchNoCore => [
                "giochi delle sue playlist non hanno ancora un core associato: avviali una volta da RetroArch e compariranno qui",
                "games of its playlists have no core assigned yet: start them once from RetroArch and they will appear here",
                "juegos de sus listas aún no tienen un núcleo asignado: inícialos una vez desde RetroArch y aparecerán aquí",
                "игр из его плейлистов пока без назначенного ядра: запустите их один раз из RetroArch, и они появятся здесь",
            ],
            T::RetroArchCoreMissing => [
                "RetroArch non ha un core associato a questo gioco: avvialo una volta da RetroArch",
                "RetroArch has no core assigned to this game: start it once from RetroArch",
                "RetroArch no tiene un núcleo asignado a este juego: inícialo una vez desde RetroArch",
                "В RetroArch этой игре не назначено ядро: запустите её один раз из RetroArch",
            ],
            T::EmulatorNotFound => [
                "Emulatore non trovato: controllalo nelle Impostazioni",
                "Emulator not found: check it in Settings",
                "Emulador no encontrado: revísalo en Ajustes",
                "Эмулятор не найден: проверьте его в настройках",
            ],
            T::ThemeInvalid => ["Tema non valido", "Invalid theme", "Tema no válido", "Некорректная тема"],
            T::ThemeNotFound => ["Tema non trovato", "Theme not found", "Tema no encontrado", "Тема не найдена"],
            T::GameNotInstalled => ["Il gioco non è installato", "The game is not installed", "El juego no está instalado", "Игра не установлена"],
            T::NoInstallTarget => ["Kanso non può avviare l'installazione per questo store: usa il suo client", "Kanso cannot start the installation for this store: use its client", "Kanso no puede iniciar la instalación para esta tienda: usa su cliente", "Kanso не может запустить установку для этого магазина: используйте его клиент"],
            T::NoUninstallTarget => ["Questo gioco non può essere disinstallato da qui: usa il client del suo store", "This game cannot be uninstalled from here: use its store's client", "Este juego no se puede desinstalar desde aquí: usa el cliente de su tienda", "Эту игру нельзя удалить отсюда: используйте клиент её магазина"],
            T::DlssNotNvidia => [
                "Il file non è una libreria DLSS firmata da NVIDIA: non è stato usato",
                "The file is not a DLSS library signed by NVIDIA: it was not used",
                "El archivo no es una biblioteca DLSS firmada por NVIDIA: no se ha usado",
                "Файл не является библиотекой DLSS с подписью NVIDIA: он не был использован",
            ],
            T::DlssNothingOlder => [
                "Nessuna libreria da aggiornare: il gioco ha già questa versione o una più recente",
                "Nothing to update: the game already has this version or a newer one",
                "Nada que actualizar: el juego ya tiene esta versión o una más reciente",
                "Обновлять нечего: в игре уже эта версия или новее",
            ],
            T::DlssBusy => [
                "Impossibile scrivere nella cartella del gioco: nulla è stato cambiato. Chiudi il gioco e riprova; se la cartella è protetta da Windows, avvia Kanso come amministratore",
                "Could not write in the game's folder: nothing was changed. Close the game and try again; if Windows protects the folder, run Kanso as administrator",
                "No se pudo escribir en la carpeta del juego: no se ha cambiado nada. Cierra el juego y vuelve a intentarlo; si Windows protege la carpeta, ejecuta Kanso como administrador",
                "Не удалось записать в папку игры: ничего не изменено. Закройте игру и повторите попытку; если папка защищена Windows, запустите Kanso от имени администратора",
            ],
            T::DlssUpdatesOff => [
                "Il download da NVIDIA è spento: si accende da Impostazioni → DLSS",
                "Downloading from NVIDIA is off: turn it on in Settings → DLSS",
                "La descarga desde NVIDIA está desactivada: se activa en Ajustes → DLSS",
                "Загрузка с NVIDIA выключена: включите её в Настройки → DLSS",
            ],
            T::DlssDownloadFailed => [
                "Download da NVIDIA non riuscito",
                "The download from NVIDIA failed",
                "La descarga desde NVIDIA ha fallado",
                "Не удалось загрузить с NVIDIA",
            ],
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
            T::SignInPortBusy => [
                "Un altro programma sta usando la porta {port} di questo PC, che serve per ricevere la risposta di Microsoft: chiudilo e riprova",
                "Another program is using port {port} of this PC, which is needed to receive Microsoft's answer: close it and try again",
                "Otro programa está usando el puerto {port} de este PC, necesario para recibir la respuesta de Microsoft: ciérralo y vuelve a intentarlo",
                "Другая программа занимает порт {port} этого компьютера, нужный для получения ответа Microsoft: закройте её и повторите попытку",
            ],
            T::BrowserSignInTimedOut => [
                "L'accesso nel browser non è stato completato in tempo",
                "The sign-in in the browser was not completed in time",
                "El inicio de sesión en el navegador no se completó a tiempo",
                "Вход в браузере не был завершён вовремя",
            ],
            T::BrowserSignInDone => [
                "Accesso riuscito. Puoi chiudere questa scheda e tornare a Kanso.",
                "Signed in. You can close this tab and go back to Kanso.",
                "Sesión iniciada. Puedes cerrar esta pestaña y volver a Kanso.",
                "Вход выполнен. Можно закрыть эту вкладку и вернуться в Kanso.",
            ],
            T::BrowserSignInRefused => [
                "Accesso non completato. Puoi chiudere questa scheda e riprovare da Kanso.",
                "The sign-in was not completed. You can close this tab and try again from Kanso.",
                "El inicio de sesión no se completó. Puedes cerrar esta pestaña y volver a intentarlo desde Kanso.",
                "Вход не завершён. Можно закрыть эту вкладку и повторить попытку из Kanso.",
            ],
            T::LibraryNotListed => [
                "Accesso riuscito, ma lo store non ha dato l'elenco dei giochi",
                "Signed in, but the store did not list the games",
                "Sesión iniciada, pero la tienda no ha dado la lista de juegos",
                "Вход выполнен, но магазин не выдал список игр",
            ],
            T::ClientGames => [
                "Giochi letti da Ubisoft Connect su questo PC: {n}",
                "Games read from Ubisoft Connect on this PC: {n}",
                "Juegos leídos de Ubisoft Connect en este PC: {n}",
                "Игр прочитано из Ubisoft Connect на этом ПК: {n}",
            ],
            T::AccountGames => ["Giochi letti dall'account: {n}", "Games read from the account: {n}", "Juegos leídos de la cuenta: {n}", "Игр прочитано из аккаунта: {n}"],
            T::AccountSubscriptionGames => [
                "di cui {n} dall'abbonamento ({total} nel suo catalogo)",
                "{n} of them through the subscription ({total} in its catalog)",
                "de ellos {n} por la suscripción ({total} en su catálogo)",
                "из них {n} по подписке ({total} в её каталоге)",
            ],
            T::AccountGamesLeftOut => [
                "{n} lasciati fuori perché non per PC ({platforms})",
                "{n} left out as not for PC ({platforms})",
                "{n} omitidos por no ser para PC ({platforms})",
                "{n} пропущено: не для ПК ({platforms})",
            ],
            T::TrayOpen => ["Apri Kanso", "Open Kanso", "Abrir Kanso", "Открыть Kanso"],
            T::TrayQuit => ["Esci", "Quit", "Salir", "Выход"],
            T::UpdateCheckFailed => [
                "Impossibile verificare gli aggiornamenti",
                "Could not check for updates",
                "No se pudo comprobar si hay actualizaciones",
                "Не удалось проверить обновления",
            ],
            T::UpdateRateLimited => [
                "GitHub ha limitato temporaneamente le richieste: riprova tra qualche minuto",
                "GitHub temporarily rate-limited requests: try again in a few minutes",
                "GitHub ha limitado temporalmente las solicitudes: inténtalo de nuevo en unos minutos",
                "GitHub временно ограничил число запросов: повторите попытку через несколько минут",
            ],
            T::UpdateDownloadFailed => [
                "Download dell'aggiornamento non riuscito",
                "Update download failed",
                "Error al descargar la actualización",
                "Не удалось загрузить обновление",
            ],
            T::UpdateChecksumMismatch => [
                "Il file scaricato non corrisponde all'impronta SHA-256 pubblicata ed è stato eliminato",
                "The downloaded file does not match the published SHA-256 checksum and was deleted",
                "El archivo descargado no coincide con la suma SHA-256 publicada y se ha eliminado",
                "Загруженный файл не совпадает с опубликованной контрольной суммой SHA-256 и был удалён",
            ],
            T::UpdateNoChecksum => [
                "Questa versione non include un'impronta SHA-256 verificabile: impossibile installarla automaticamente",
                "This release has no verifiable SHA-256 checksum: cannot install it automatically",
                "Esta versión no incluye una suma SHA-256 verificable: no se puede instalar automáticamente",
                "У этого выпуска нет проверяемой контрольной суммы SHA-256: автоматическая установка невозможна",
            ],
            T::UpdateNoInstaller => [
                "Nessun programma di installazione compatibile trovato in questa versione",
                "No compatible installer found in this release",
                "No se encontró un instalador compatible en esta versión",
                "В этом выпуске не найден подходящий установщик",
            ],
            T::UpdateInstallFailed => [
                "Impossibile avviare il programma di installazione",
                "Could not start the installer",
                "No se pudo iniciar el instalador",
                "Не удалось запустить установщик",
            ],
            T::UpdateCancelled => [
                "Aggiornamento annullato",
                "Update cancelled",
                "Actualización cancelada",
                "Обновление отменено",
            ],
            T::UpdateInProgress => [
                "Un aggiornamento è già in corso",
                "An update is already in progress",
                "Ya hay una actualización en curso",
                "Обновление уже выполняется",
            ],
            T::UpdateVersionMismatch => [
                "Nessun aggiornamento verificato corrisponde alla versione richiesta: riprova la verifica",
                "No verified update matches the requested version: check for updates again",
                "Ninguna actualización verificada coincide con la versión solicitada: vuelve a comprobar",
                "Нет проверенного обновления для запрошенной версии: проверьте обновления ещё раз",
            ],
            T::UpdateWindowsOnly => [
                "L'installazione automatica degli aggiornamenti è disponibile solo su Windows",
                "Automatic update installation is only available on Windows",
                "La instalación automática de actualizaciones solo está disponible en Windows",
                "Автоматическая установка обновлений доступна только в Windows",
            ],
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
