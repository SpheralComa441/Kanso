/**
 * What moves behind the top of the Home page: something of the season, of a holiday when one
 * is near, or of the weather outside when the user asked Kanso to follow it.
 *
 * Deciding is plain arithmetic on the date; nothing here contacts anything. The weather, when
 * there is one, is handed in by the caller.
 */

/** What falls from the top of the page. */
export type AmbienceKind =
  | "snow"
  | "rain"
  | "storm"
  | "fog"
  | "petals"
  | "motes"
  | "leaves"
  | "confetti"
  | "hearts"
  | "festive"
  | "spooky"
  | "pastel";

/** Why: a season, a holiday, or the weather. Also the key of what the page says about it. */
export type AmbienceReason =
  | "winter"
  | "spring"
  | "summer"
  | "autumn"
  | "christmas"
  | "newYear"
  | "halloween"
  | "valentine"
  | "easter"
  | "weatherSnow"
  | "weatherRain"
  | "weatherStorm"
  | "weatherFog";

export interface Ambience {
  kind: AmbienceKind;
  reason: AmbienceReason;
}

/** The weather right now, as Open-Meteo codes it (WMO weather codes). */
export interface WeatherNow {
  code: number;
  isDay: boolean;
}

/** Easter Sunday of a year (Gregorian calendar), as month (1–12) and day. */
export function easterSunday(year: number): { month: number; day: number } {
  const a = year % 19;
  const b = Math.floor(year / 100);
  const c = year % 100;
  const d = Math.floor(b / 4);
  const e = b % 4;
  const f = Math.floor((b + 8) / 25);
  const g = Math.floor((b - f + 1) / 3);
  const h = (19 * a + b - d - g + 15) % 30;
  const i = Math.floor(c / 4);
  const k = c % 4;
  const l = (32 + 2 * e + 2 * i - h - k) % 7;
  const m = Math.floor((a + 11 * h + 22 * l) / 451);
  const month = Math.floor((h + l - 7 * m + 114) / 31);
  const day = ((h + l - 7 * m + 114) % 31) + 1;
  return { month, day };
}

/** The holiday a date is on or near, if any. */
function holidayOf(date: Date): Ambience | null {
  const month = date.getMonth() + 1;
  const day = date.getDate();
  if ((month === 12 && day >= 30) || (month === 1 && day === 1)) return { kind: "confetti", reason: "newYear" };
  if (month === 12 && day >= 18 && day <= 26) return { kind: "festive", reason: "christmas" };
  if (month === 10 && day >= 25) return { kind: "spooky", reason: "halloween" };
  if (month === 2 && (day === 13 || day === 14)) return { kind: "hearts", reason: "valentine" };
  // From Good Friday to Easter Monday
  const easter = easterSunday(date.getFullYear());
  const sunday = new Date(date.getFullYear(), easter.month - 1, easter.day).getTime();
  const today = new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
  const daysFromEaster = Math.round((today - sunday) / 86_400_000);
  if (daysFromEaster >= -2 && daysFromEaster <= 1) return { kind: "pastel", reason: "easter" };
  return null;
}

/** What the weather shows, when it shows something: rain, snow, a storm, fog. */
function weatherOf(weather: WeatherNow): Ambience | null {
  const code = weather.code;
  if ((code >= 71 && code <= 77) || code === 85 || code === 86) return { kind: "snow", reason: "weatherSnow" };
  if (code >= 95 && code <= 99) return { kind: "storm", reason: "weatherStorm" };
  if ((code >= 51 && code <= 67) || (code >= 80 && code <= 82)) return { kind: "rain", reason: "weatherRain" };
  if (code === 45 || code === 48) return { kind: "fog", reason: "weatherFog" };
  // A clear or cloudy sky leaves the season to show
  return null;
}

function seasonOf(date: Date, southern: boolean): Ambience {
  // Seasons by the month, as weather services count them; half a year apart down south
  const month = (date.getMonth() + (southern ? 6 : 0)) % 12;
  if (month === 11 || month <= 1) return { kind: "snow", reason: "winter" };
  if (month <= 4) return { kind: "petals", reason: "spring" };
  if (month <= 7) return { kind: "motes", reason: "summer" };
  return { kind: "leaves", reason: "autumn" };
}

/** Time zones of the southern half of the world, where the seasons are the other way round. */
const SOUTHERN_ZONES =
  /^(Australia\/|Antarctica\/|Pacific\/(Auckland|Chatham|Fiji|Tongatapu|Apia|Port_Moresby|Noumea)|America\/(Argentina\/|Sao_Paulo|Santiago|Montevideo|Asuncion|La_Paz|Lima|Buenos_Aires|Punta_Arenas)|Africa\/(Johannesburg|Maputo|Harare|Windhoek|Gaborone|Lusaka|Maseru|Mbabane|Luanda|Dar_es_Salaam)|Indian\/(Mauritius|Reunion|Antananarivo))/;

/** Whether this PC is, as far as its time zone says, south of the equator. */
export function isSouthernTimeZone(timeZone: string): boolean {
  return SOUTHERN_ZONES.test(timeZone);
}

/**
 * What the Home page shows behind its top on `date`. A holiday comes first; then the weather,
 * when the user follows it and it is doing something; then the season.
 */
export function ambienceFor(date: Date, southern: boolean, weather: WeatherNow | null): Ambience {
  return holidayOf(date) ?? (weather ? weatherOf(weather) : null) ?? seasonOf(date, southern);
}
