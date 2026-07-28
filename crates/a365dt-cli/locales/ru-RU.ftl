test-value = Значение: { $value }

language-unsupported = Язык { $language } не поддерживается.
language-use-english = Продолжить на английском?
language-invalid = Недопустимое значение языка: { $language }.
language-missing = Для `--lang` нужно указать язык.
language-suggestion = Возможно, вы имели в виду: { $language }
language-error-with-suggestion =
    { $message }
    Возможно, вы имели в виду: { $suggestion }

cli-about = Скачивайте эпизоды Anime365 с нужным переводом
cli-command-cache = Управление локальным кешем
cli-command-cache-prune = Очистить локальный кеш
cli-command-completions = Создать автодополнения для командной оболочки
cli-command-doctor = Проверить состояние a365dt, Anime365, кеша и телеметрии
cli-command-purge = Безвозвратно удалить все локальные данные a365dt
cli-command-telemetry = Просмотр и управление локальной телеметрией
cli-command-telemetry-clear = Очистить телеметрию, не меняя ее состояние
cli-command-telemetry-disable = Остановить сбор локальной телеметрии
cli-command-telemetry-enable = Возобновить сбор локальной телеметрии
cli-command-telemetry-show = Показать все собранные поля и их значения
cli-command-help = Показать это сообщение или справку по указанной подкоманде
cli-option-query-or-url = Название тайтла или URL каталога Anime365
cli-option-query = Искать тайтл, даже если название совпадает с командой
cli-option-output = Каталог для загрузки
cli-option-jobs = Количество одновременных загрузок
cli-option-debug = Показать технические подробности ошибок
cli-option-lang = Выбрать язык для этого запуска
cli-option-help = Показать справку
cli-option-version = Показать версию
cli-option-completion-shell = Командная оболочка для автодополнений
cli-option-purge-yes = Удалить данные без подтверждения

cli-help-root =
    Скачивайте эпизоды Anime365 с нужным переводом
    Использование: a365dt [OPTIONS] [QUERY_OR_URL]... [COMMAND]
    Команды:
      cache         Управление локальным кешем
      completions   Создать автодополнения для командной оболочки
      doctor        Проверить состояние a365dt, Anime365, кеша и телеметрии
      purge         Безвозвратно удалить все локальные данные a365dt
      telemetry     Просмотр и управление локальной телеметрией
      help          Показать справку по команде
    Параметры:
      --query <QUERY>...       Искать, даже если тайтл совпадает с командой
      -o, --output <DIR>       Каталог для загрузки [по умолчанию: .]
      -j, --jobs <JOBS>        Одновременные загрузки [по умолчанию: 4]
          --debug              Показать технические подробности ошибок
          --lang <LANG>        Язык для этого запуска
      -h, --help               Показать справку
      -V, --version            Показать версию
cli-help-cache =
    Управление локальным кешем
    Использование: a365dt cache <COMMAND>
    Команды:
      prune   Очистить локальный кеш
      help    Показать справку по команде
    Параметры:
          --lang <LANG>   Язык для этого запуска
      -h, --help          Показать справку
      -V, --version       Показать версию
cli-help-cache-prune =
    Очистить локальный кеш
    Использование: a365dt cache prune [OPTIONS]
    Параметры:
          --lang <LANG>   Язык для этого запуска
          --debug         Показать технические подробности ошибок
      -h, --help          Показать справку
      -V, --version       Показать версию
cli-help-completions =
    Создать автодополнения для командной оболочки
    Использование: a365dt completions <SHELL>
    Аргументы:
      <SHELL>   Командная оболочка для автодополнений
    Параметры:
          --lang <LANG>   Язык для этого запуска
          --debug         Показать технические подробности ошибок
      -h, --help          Показать справку
      -V, --version       Показать версию
cli-help-doctor =
    Проверить состояние a365dt, Anime365, кеша и телеметрии
    Использование: a365dt doctor [OPTIONS]
    Параметры:
          --debug         Показать технические подробности ошибок
          --lang <LANG>   Язык для этого запуска
      -h, --help          Показать справку
      -V, --version       Показать версию
cli-help-purge =
    Безвозвратно удалить все локальные данные a365dt
    Использование: a365dt purge [OPTIONS]
    Параметры:
      -y, --yes           Удалить данные без подтверждения
          --debug         Показать технические подробности ошибок
          --lang <LANG>   Язык для этого запуска
      -h, --help          Показать справку
      -V, --version       Показать версию
cli-help-telemetry =
    Просмотр и управление локальной телеметрией
    Использование: a365dt telemetry <COMMAND>
    Команды:
      clear     Очистить собранную телеметрию
      disable   Остановить сбор локальной телеметрии
      enable    Возобновить сбор локальной телеметрии
      show      Показать все собранные поля
      help      Показать справку по команде
    Параметры:
          --lang <LANG>   Язык для этого запуска
      -h, --help          Показать справку
      -V, --version       Показать версию
cli-help-telemetry-clear =
    Очистить телеметрию, не меняя ее состояние
    Использование: a365dt telemetry clear [OPTIONS]
    Параметры:
          --lang <LANG>   Язык для этого запуска
          --debug         Показать технические подробности ошибок
      -h, --help          Показать справку
      -V, --version       Показать версию
cli-help-telemetry-disable =
    Остановить сбор локальной телеметрии
    Использование: a365dt telemetry disable [OPTIONS]
    Параметры:
          --lang <LANG>   Язык для этого запуска
          --debug         Показать технические подробности ошибок
      -h, --help          Показать справку
      -V, --version       Показать версию
cli-help-telemetry-enable =
    Возобновить сбор локальной телеметрии
    Использование: a365dt telemetry enable [OPTIONS]
    Параметры:
          --lang <LANG>   Язык для этого запуска
          --debug         Показать технические подробности ошибок
      -h, --help          Показать справку
      -V, --version       Показать версию
cli-help-telemetry-show =
    Показать все собранные поля и их значения
    Использование: a365dt telemetry show [OPTIONS]
    Параметры:
          --lang <LANG>   Язык для этого запуска
          --debug         Показать технические подробности ошибок
      -h, --help          Показать справку
      -V, --version       Показать версию

cli-error-conflict = Аргумент { $argument } нельзя использовать вместе с { $other }.
cli-error-subcommand = Неизвестная подкоманда: { $value }.
cli-error-value = Недопустимое значение { $value } для { $argument }.
cli-error-required = Не указан обязательный аргумент: { $argument }.
cli-error-missing-subcommand = Для команды { $command } нужна подкоманда.
cli-error-equals = Укажите значение { $argument } после знака равенства.
cli-error-too-many = Слишком много значений для { $argument }.
cli-error-too-few = Недостаточно значений для { $argument }.
cli-error-argument = Неожиданный аргумент: { $argument }.
cli-error-utf8 = Аргумент содержит недопустимый UTF-8.
cli-error-generic = Недопустимая командная строка.
cli-error-with-help =
    { $message }
    Используйте `a365dt --help` для справки.
cli-error-with-suggestion =
    { $message }
    Возможно, вы имели в виду: { $suggestion }
    Используйте `a365dt --help` для справки.

cancelled = Отменено.
interrupted = Прервано.
error-context = { $context }: { $message }
ui-prompt-display-error = Не удалось показать приглашение для ввода.
ui-prompt-read-error = Не удалось прочитать ввод из терминала.
ui-input-closed = Ввод закрыт до получения ответа.
ui-secret-display-error = Не удалось показать приглашение для секретного ввода.
ui-secret-read-error = Не удалось прочитать секретный ввод из терминала.
ui-confirm-yes-no = Введите yes или no.
selector-no-choices = Нет доступных вариантов.
selector-choose-range = Выберите число от 1 до { $last }:
selector-choose-listed = Выберите одно из указанных чисел.
selector-position-empty = 0 из 0
selector-position = { $first }–{ $last } из { $total }
selector-no-matches = Нет совпадений
selector-filter = Фильтр или #номер:
selector-terminal-error = Не удалось использовать интерактивный терминал.

query-command-conflict = `--query` нельзя использовать вместе с командой. Удалите команду или поисковый запрос.
purge-confirm = Безвозвратно удалить все локальные данные a365dt и сохраненные учетные данные?
purge-cancelled = Удаление отменено.
purge-error = Не удалось удалить все локальные файлы a365dt.
purge-success = Локальные данные a365dt удалены
ctrl-c-listen-error = Не удалось обработать Ctrl+C.
app-heading = a365dt  ◆  загрузчик Anime365
cache-clear-error = Не удалось очистить локальный кеш.
cache-clear-success = Локальный кеш очищен
auth-validating = Проверка доступа к Anime365…
auth-success = Вход выполнен
series-selected = Выбран тайтл: { $title }
translation-selected = Выбран перевод { $kind }-{ $language }, авторы: { $authors }
media-loading = Загрузка доступных медиа…
subtitles-embedded =
    { $count ->
        [one] В 1 эпизоде субтитры находятся внутри MP4.
        [few] В { $count } эпизодах субтитры находятся внутри MP4.
       *[many] В { $count } эпизодах субтитры находятся внутри MP4.
    }
mux-confirm = Объединить видео и отдельные ASS-субтитры в MKV после загрузки?
mux-unavailable = ffmpeg недоступен; MP4 и ASS останутся отдельными файлами.
output-create-error = Не удалось создать каталог { $path }.
output-directory = Каталог загрузки: { $path }
media-task-error = Внутренняя задача остановилась при загрузке медиа эпизода.
summary-heading = Итоги загрузки
summary-downloaded = Скачано
summary-skipped = Пропущено
summary-failed = Ошибки
summary-interrupted = Прервано
summary-size = Размер
summary-elapsed = Время
summary-output = Каталог
summary-error = { $episode }: { $error }
summary-resume = Запустите ту же команду снова, чтобы продолжить сохраненные файлы .part.
command-suggestions =
    Неизвестная команда или подкоманда.
    Возможно, вы имели в виду:{ $commands }
    Используйте `--query`, чтобы искать введенные слова как название тайтла.

auth-keychain-token = Используется токен доступа Anime365 из Связки ключей macOS.
auth-token-unavailable =
    Токен доступа Anime365 не найден, а a365dt не может запросить его здесь.
    Запустите a365dt в интерактивном терминале или передайте токен через переменную окружения процесса ANIME365_ACCESS_TOKEN.
auth-opening = Открывается { $url }
auth-browser-error = Не удалось открыть браузер автоматически.
auth-sign-in = Если Anime365 требует авторизацию, войдите и обновите страницу.
auth-token-prompt = Вставьте токен доступа:
auth-token-empty = Токен доступа Anime365 не может быть пустым.
auth-keychain-read-error-detail = Не удалось прочитать токен доступа Anime365 из Связки ключей macOS: { $error }
auth-keychain-save-confirm = Сохранить этот токен в Связке ключей macOS?
auth-keychain-save-error = Не удалось сохранить токен доступа Anime365 в Связке ключей macOS.
auth-keychain-save-success = Токен доступа сохранен в Связке ключей macOS.
auth-keychain-remove-error = Не удалось удалить токен доступа Anime365 из Связки ключей macOS.

api-client-init-error = Не удалось инициализировать защищенный HTTP-клиент.
api-too-many-translations = Anime365 вернул слишком много переводов.
api-missing-data = Anime365 не вернул запрошенные данные API.
api-request-error = Ошибка запроса к API Anime365.
api-response-error = a365dt не удалось прочитать ответ Anime365.
api-service-error = Ошибка Anime365 { $code }: { $message }
api-status-error = Anime365 отклонил запрос API (HTTP { $status }).
api-invalid-media-url = Anime365 вернул недопустимый URL медиа.
api-invalid-poster-url = Anime365 вернул недопустимый URL постера.
api-media-request-error = Ошибка запроса к медиасерверу Anime365.

select-no-series = Подходящие тайтлы Anime365 не найдены.
select-search-results = Результаты поиска
select-unknown-type = Неизвестный тип
select-episodes-unknown = ? эпизодов
select-episodes =
    { $count ->
        [one] 1 эпизод
        [few] { $count } эпизода
       *[many] { $count } эпизодов
    }
select-episode-ranges = Диапазоны эпизодов (примеры: 1-12,16-18; 0-12.5; 5):
select-unavailable-episodes = Недоступные эпизоды: { $episodes }
select-missing-action = Как продолжить?
select-continue-available = Продолжить с доступными эпизодами
select-enter-different = Ввести другой диапазон
select-cancel = Отменить
select-fractional-confirm = Включить дробные эпизоды { $episodes }?
select-empty = В выбранном диапазоне нет эпизодов.
select-no-translations = Ни один перевод не покрывает выбранные эпизоды.
select-coverage = { $count }/{ $total } эпизодов
select-translations = Переводы
select-skip-missing-confirm = Скачать только доступные эпизоды и пропустить { $episodes }?
select-no-resolutions = Anime365 не вернул доступных разрешений для загрузки.
select-preferred-resolution = Предпочтительное разрешение
select-no-resolution-for = Нет доступного разрешения для эпизодов { $episodes }.
select-resolution-fallback = Другое разрешение для эпизодов { $episodes }
select-missing-media-url = Anime365 не вернул URL медиа { $height }p
select-invalid-range = Введите возрастающие диапазоны не шире 10 000 эпизодов после объединения пересечений.
select-invalid-episode-number = Номера эпизодов должны быть неотрицательными числами.

search-prompt = Введите название тайтла или URL каталога Anime365:
search-cached-missing = Этого сохраненного тайтла Anime365 больше нет.
search-label = Поиск тайтла или URL Anime365
search-loading = Загрузка тайтла…
search-removed-missing = Этого сохраненного тайтла больше нет; он удален из кеша.
search-input-task-error = Задача ввода из терминала остановилась.
search-invalid-url = Введите официальный URL тайтла из каталога Anime365.
search-series-missing = Этого тайтла Anime365 больше нет.

tip-label = Совет
tip-query = Используйте `--query`, если название тайтла совпадает с командой.
tip-doctor = Запустите `a365dt doctor`, чтобы проверить Anime365, кеш, телеметрию и сборку.
tip-ffmpeg = Установите [FFmpeg](https://ffmpeg.org/), чтобы объединять отдельные ASS-субтитры с видео в MKV.
tip-resume = Запустите ту же команду снова, чтобы продолжить сохраненные загрузки `.part`.
tip-jobs = Используйте `--jobs N`, чтобы изменить количество одновременных загрузок.
tip-telemetry = Используйте `a365dt telemetry show`, чтобы просмотреть локально собранные поля.
tip-output = Используйте `--output DIR`, чтобы выбрать каталог загрузки.
tip-cow = Уровня с коровой нет.
upgrade-available = 💫 Доступно обновление:
upgrade-label = Обновить
upgrade-manual = Скачайте { $url } и замените { $executable }.

download-task = Задача загрузки
download-task-error = Внутренняя задача загрузки неожиданно остановилась.
download-stopping = Завершение работы; сохранение частичных файлов…
download-cancel-channel-error = Канал отмены закрыт; активные загрузки останавливаются.
download-not-started = Не начато, так как загрузка была прервана.
download-mkv-exists = MKV уже существует.
download-refresh-failed = { $episode } • не удалось обновить ссылку: { $error }
download-partial-saved = Загрузка прервана; частичный файл сохранен для продолжения.
download-retry = { $episode } • повтор { $attempt }/{ $retries }
download-subtitle-failed = Не удалось скачать субтитры
download-muxing = { $episode } • объединение в MKV
download-resume-size-missing = Медиасервер не сообщил размер файла, необходимый для продолжения загрузки.
download-network-interrupted = Загрузка медиа прервана из-за сетевой ошибки.
download-invalid-resume = Медиасервер вернул недопустимые данные для продолжения загрузки.
download-invalid-content-range = недопустимый Content-Range: { $value }
download-http-rejected = Медиасервер отклонил загрузку (HTTP { $status }).
download-empty-file = Медиасервер вернул пустой файл.
download-incomplete-file = Файл загружен не полностью: { $received } из { $expected } байт.
download-file-io-error = Не удалось прочитать или записать файл загрузки.
progress-episodes = эпизодов
progress-batch = Загрузка
progress-eta = Осталось
mux-start-error = Не удалось запустить ffmpeg.
mux-combine-error = ffmpeg не удалось объединить видео и субтитры.
mux-verify-error = Не удалось проверить файл MKV.
mux-empty-error = ffmpeg создал пустой файл MKV.
mux-write-error = Не удалось завершить запись файла MKV.
mux-save-error = Не удалось сохранить файл MKV.
mux-remove-video-error = MKV сохранен, но исходное видео не удалось удалить.
mux-remove-subtitle-error = MKV сохранен, но исходные субтитры не удалось удалить.

unavailable = Недоступно
unavailable-no-observations = Недоступно: нет наблюдений
never = Никогда
telemetry-cleared = Локальная телеметрия очищена
telemetry-disabled = Локальная телеметрия отключена
telemetry-enabled = Локальная телеметрия включена
telemetry-directory-error = Не удалось определить каталог локальной телеметрии.
telemetry-directory-create-error = Не удалось создать каталог локальной телеметрии.
telemetry-lock-open-error = Не удалось открыть блокировку локальной телеметрии.
telemetry-lock-error = Не удалось заблокировать локальную телеметрию.
telemetry-read-error = Не удалось прочитать локальную телеметрию.
telemetry-invalid-error = Локальная телеметрия повреждена. Выполните `a365dt telemetry clear`, чтобы сбросить ее.
telemetry-schema-unsupported = Схема локальной телеметрии { $version } не поддерживается. Выполните `a365dt telemetry clear`, чтобы сбросить ее.
telemetry-prepare-error = Не удалось подготовить локальную телеметрию.
telemetry-store-error = Не удалось сохранить локальную телеметрию.
telemetry-opt-out-inspect-error = Не удалось проверить отключение локальной телеметрии.
telemetry-opt-out-directory-error = Не удалось определить каталог отключения локальной телеметрии.
telemetry-opt-out-create-error = Не удалось создать каталог отключения локальной телеметрии.
telemetry-disable-error = Не удалось отключить локальную телеметрию.
telemetry-enable-error = Не удалось включить локальную телеметрию.
telemetry-data-inspect-error = Не удалось проверить данные локальной телеметрии.
telemetry-heading = Локальная телеметрия
telemetry-collection = Сбор
telemetry-state-disabled = Отключен
telemetry-state-enabled = Включен
telemetry-data = Данные
telemetry-opt-out = Отключение
telemetry-schema = Схема
telemetry-first-observation = Первое наблюдение
telemetry-last-observation = Последнее наблюдение
telemetry-last-enabled = Последнее включение
telemetry-last-disabled = Последнее отключение
telemetry-last-cleared = Последняя очистка
telemetry-first-download = Первая загрузка
telemetry-last-download = Последняя загрузка
telemetry-counters-heading = Собранные счетчики
telemetry-no-counters = Счетчики не записаны
telemetry-statistics-heading = Рассчитанная статистика
telemetry-performance-heading = Наблюдения производительности
telemetry-no-performance = Наблюдения производительности не записаны
telemetry-operation = Операция
telemetry-count = Количество
telemetry-total = Всего
telemetry-average = Среднее
telemetry-median = Медиана
telemetry-work-units = Единицы работы
telemetry-samples-heading = Последние образцы
telemetry-no-samples = Образцы не записаны
telemetry-metric = Метрика
telemetry-samples = Образцы

doctor-heading = Проверка a365dt
doctor-section-health = Состояние
doctor-section-statistics = Статистика
doctor-section-build = Сборка
doctor-section-debug = Отладочная диагностика
doctor-status-line = { $symbol } { $status }
doctor-status-healthy = Исправно
doctor-status-warning = Требуется внимание
doctor-status-error = Неисправно
doctor-value-remedy = { $value } — { $remedy }
doctor-remedy-server-error = Проверьте сеть или состояние Anime365 и повторите попытку
doctor-remedy-server-slow = Повторите попытку; если задержка останется высокой, проверьте сеть
doctor-series-cache = Кеш тайтлов
doctor-fresh = Актуален
doctor-stale = Устарел
doctor-not-created = Еще не создан
doctor-unreadable = Не читается
doctor-remedy-refresh-cache = Выполните поиск тайтла, чтобы обновить кеш
doctor-remedy-create-cache = Выполните поиск тайтла, чтобы создать кеш
doctor-remedy-reset-cache = Выполните `a365dt cache prune`, чтобы сбросить кеш
doctor-remedy-enable-telemetry = Выполните `a365dt telemetry enable`, чтобы возобновить наблюдения
doctor-remedy-reset-telemetry = Выполните `a365dt telemetry clear`, чтобы сбросить телеметрию
doctor-catalogue-hit-rate = Доля попаданий в каталог
doctor-api-requests = Запросы API
doctor-media-requests = Запросы медиа
doctor-cache-retrieval = Чтение кеша
doctor-search = Поиск
doctor-search-throughput = Скорость поиска
doctor-downloads = Загрузки
doctor-download-volume = Объем загрузок
doctor-command-usage = Использование команд
doctor-remedy-reset-observations = Сбросьте локальную телеметрию и соберите новые наблюдения
doctor-historical = {" (исторические данные)"}
doctor-search-rate = { $rate } тайтлов/с{ $suffix }
doctor-remedy-run-searches = Выполните поиск при включенной телеметрии
doctor-remedy-run-downloads = Выполните загрузку при включенной телеметрии
doctor-download-volume-value = { $batches } загрузок · { $episodes } эпизодов · { $bytes }{ $suffix }
doctor-command-count = { $commands } команд{ $suffix }
doctor-last-cache-update = Последнее обновление кеша
doctor-cached-series = Тайтлов в кеше
doctor-remedy-cache-prune = Выполните `a365dt cache prune`
doctor-version = Версия
doctor-commit = Коммит
doctor-profile = Профиль
doctor-platform = Платформа
doctor-compiler = Компилятор
doctor-server-endpoint = Адрес сервера
doctor-server-response = Ответ сервера
doctor-server-response-value = { $status } · { $latency }
doctor-no-http-response = Нет ответа HTTP
doctor-latency-threshold = Порог предупреждения о задержке
doctor-server-detail = Подробности сервера
doctor-cache-age = Возраст: { $age } · TTL: { $ttl }
doctor-missing = Отсутствует
doctor-missing-lowercase = отсутствует
doctor-cache-path = Путь к кешу
doctor-cache-detail = Подробности кеша
doctor-telemetry-data-value = { $path } · { $size }
doctor-operation-latency = Задержка по операциям
doctor-remedy-collect-telemetry = Соберите телеметрию с помощью поиска или загрузок
doctor-latency-operation = Задержка · { $operation }
doctor-usage-counters = Счетчики использования
doctor-remedy-run-commands = Выполняйте команды при включенной телеметрии
doctor-counter = Счетчик · { $counter }
doctor-telemetry-detail = Подробности телеметрии
doctor-telemetry-overhead = Накладные расходы телеметрии
doctor-telemetry-overhead-value = включено: { $enabled } нс · отключено: { $disabled } нс · добавлено: { $added } нс
doctor-performance-value = среднее: { $average } · медиана: { $median } · наблюдений: { $count }{ $suffix }
doctor-remedy-run-activity = Выполняйте поиск или загрузки при включенной телеметрии
doctor-performance-detail = среднее: { $average } · медиана: { $median } · всего: { $total } · образцов: { $samples } · единиц работы: { $work_units }
doctor-rate-value = { $percent }% · наблюдений: { $total }{ $suffix }
doctor-server-http-unavailable = Недоступен (HTTP { $status })
doctor-server-read-error = Не удалось прочитать ответ
doctor-server-available = Доступен · { $latency }
doctor-server-available-slow = Доступен · { $latency } · высокая задержка
doctor-server-timeout = Недоступен · превышено время ожидания
doctor-server-request-error = Недоступен · ошибка запроса
doctor-cache-directory-error = Не удалось определить системный каталог кеша.
