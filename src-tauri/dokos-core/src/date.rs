//! Dates et dates-heures au format attendu par Frappe, sans fuseau.

use std::fmt;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErreurDate(pub String);

impl fmt::Display for ErreurDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "date invalide : « {} » (formats acceptés : 2026-11-12 ou 12/11/2026, heure 14:30)", self.0)
    }
}

impl std::error::Error for ErreurDate {}

/// Date du calendrier. L'ordre des champs donne l'ordre chronologique.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    annee: u16,
    mois: u8,
    jour: u8,
}

impl Date {
    pub fn new(annee: u16, mois: u8, jour: u8) -> Result<Self, ErreurDate> {
        let invalide = || ErreurDate(format!("{annee:04}-{mois:02}-{jour:02}"));
        if !(1900..=2100).contains(&annee) || !(1..=12).contains(&mois) {
            return Err(invalide());
        }
        if jour == 0 || jour > jours_dans_mois(annee, mois) {
            return Err(invalide());
        }
        Ok(Self { annee, mois, jour })
    }

    pub fn annee(&self) -> u16 {
        self.annee
    }

    pub fn mois(&self) -> u8 {
        self.mois
    }

    pub fn jour(&self) -> u8 {
        self.jour
    }

    /// Date du jour, en temps universel (pas de fuseau sans dépendance :
    /// un décalage d'un jour est possible dans les deux heures autour de minuit).
    pub fn aujourdhui() -> Self {
        let secondes = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let jours = i64::try_from(secondes / 86_400).unwrap_or(0);
        Self::depuis_jours_epoch(jours).unwrap_or(Self { annee: 2100, mois: 12, jour: 31 })
    }

    /// Nombre de jours depuis le 1er janvier 1970 (calendrier grégorien proleptique).
    pub fn jours_epoch(&self) -> i64 {
        let (mut a, m, j) = (i64::from(self.annee), i64::from(self.mois), i64::from(self.jour));
        if m <= 2 {
            a -= 1;
        }
        let ere = a.div_euclid(400);
        let aoe = a - ere * 400;
        let mp = if m > 2 { m - 3 } else { m + 9 };
        let joa = (153 * mp + 2) / 5 + j - 1;
        let jde = aoe * 365 + aoe / 4 - aoe / 100 + joa;
        ere * 146_097 + jde - 719_468
    }

    /// Date à partir d'un nombre de jours depuis le 1er janvier 1970.
    pub fn depuis_jours_epoch(jours: i64) -> Result<Self, ErreurDate> {
        let z = jours + 719_468;
        let ere = z.div_euclid(146_097);
        let jde = z - ere * 146_097;
        let aoe = (jde - jde / 1460 + jde / 36_524 - jde / 146_096) / 365;
        let joa = jde - (365 * aoe + aoe / 4 - aoe / 100);
        let mp = (5 * joa + 2) / 153;
        let j = joa - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let a = aoe + ere * 400 + i64::from(m <= 2);
        let invalide = || ErreurDate(format!("{jours} jours"));
        Self::new(
            u16::try_from(a).map_err(|_| invalide())?,
            u8::try_from(m).map_err(|_| invalide())?,
            u8::try_from(j).map_err(|_| invalide())?,
        )
    }

    /// Date décalée de `jours` (négatif pour reculer).
    pub fn plus_jours(&self, jours: i64) -> Result<Self, ErreurDate> {
        Self::depuis_jours_epoch(self.jours_epoch() + jours)
    }

    /// Nombre de jours de `self` à `autre` (négatif si `autre` est avant).
    pub fn jours_jusqua(&self, autre: Date) -> i64 {
        autre.jours_epoch() - self.jours_epoch()
    }

    /// Jour de la semaine en toutes lettres (calendrier grégorien).
    pub fn jour_semaine(&self) -> &'static str {
        const DECALAGES: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
        let mut a = i32::from(self.annee);
        if self.mois < 3 {
            a -= 1;
        }
        let indice = (a + a / 4 - a / 100 + a / 400 + DECALAGES[usize::from(self.mois - 1)] + i32::from(self.jour)) % 7;
        ["dimanche", "lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi"][indice as usize]
    }

    /// Nombre d'années pleines entre `self` et `plus_tard` (âge à une date).
    pub fn annees_pleines_jusqua(&self, plus_tard: Date) -> i32 {
        let mut annees = i32::from(plus_tard.annee) - i32::from(self.annee);
        if (plus_tard.mois, plus_tard.jour) < (self.mois, self.jour) {
            annees -= 1;
        }
        annees
    }
}

fn jours_dans_mois(annee: u16, mois: u8) -> u8 {
    match mois {
        2 if (annee.is_multiple_of(4) && !annee.is_multiple_of(100)) || annee.is_multiple_of(400) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

impl FromStr for Date {
    type Err = ErreurDate;

    /// Accepte `AAAA-MM-JJ` et `JJ/MM/AAAA`.
    fn from_str(texte: &str) -> Result<Self, Self::Err> {
        let invalide = || ErreurDate(texte.to_owned());
        let texte_net = texte.trim();
        let morceaux: Vec<&str> =
            if texte_net.contains('-') { texte_net.split('-').collect() } else { texte_net.split('/').rev().collect() };
        let [a, m, j] = morceaux.as_slice() else {
            return Err(invalide());
        };
        let (Some(annee), Some(mois), Some(jour)) = (chiffres(a, &[4]), chiffres(m, &[1, 2]), chiffres(j, &[1, 2]))
        else {
            return Err(invalide());
        };
        let annee = u16::try_from(annee).map_err(|_| invalide())?;
        let mois = u8::try_from(mois).map_err(|_| invalide())?;
        let jour = u8::try_from(jour).map_err(|_| invalide())?;
        Date::new(annee, mois, jour).map_err(|_| invalide())
    }
}

impl fmt::Display for Date {
    /// Format Frappe : `AAAA-MM-JJ`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.annee, self.mois, self.jour)
    }
}

/// Date et heure à la minute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DateHeure {
    date: Date,
    heure: u8,
    minute: u8,
}

impl DateHeure {
    pub fn new(date: Date, heure: u8, minute: u8) -> Result<Self, ErreurDate> {
        if heure > 23 || minute > 59 {
            return Err(ErreurDate(format!("{date} {heure:02}:{minute:02}")));
        }
        Ok(Self { date, heure, minute })
    }

    pub fn date(&self) -> Date {
        self.date
    }
}

impl FromStr for DateHeure {
    type Err = ErreurDate;

    /// Une date (voir [`Date`]) puis une heure `14:30`, `14h30` ou `14h`,
    /// séparées par une espace ou un `T` (format des champs HTML `datetime-local`).
    fn from_str(texte: &str) -> Result<Self, Self::Err> {
        let invalide = || ErreurDate(texte.to_owned());
        let texte_net = texte.trim();
        let (date, heure) = texte_net.split_once('T').or_else(|| texte_net.split_once(' ')).ok_or_else(invalide)?;
        let date: Date = date.parse().map_err(|_| invalide())?;
        let heure = heure.trim().to_lowercase();
        let (h, m) = heure.split_once(':').or_else(|| heure.split_once('h')).unwrap_or((heure.as_str(), ""));
        // `14:30:00` : les secondes, s'il y en a, doivent être nulles.
        let (m, s) = m.split_once(':').unwrap_or((m, "00"));
        if s != "00" {
            return Err(invalide());
        }
        let h = chiffres(h, &[1, 2]).ok_or_else(invalide)?;
        let m = if m.is_empty() { 0 } else { chiffres(m, &[2]).ok_or_else(invalide)? };
        let h = u8::try_from(h).map_err(|_| invalide())?;
        let m = u8::try_from(m).map_err(|_| invalide())?;
        DateHeure::new(date, h, m).map_err(|_| invalide())
    }
}

impl fmt::Display for DateHeure {
    /// Format Frappe : `AAAA-MM-JJ HH:MM:SS`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {:02}:{:02}:00", self.date, self.heure, self.minute)
    }
}

fn chiffres(s: &str, longueurs: &[usize]) -> Option<u32> {
    if longueurs.contains(&s.len()) && s.bytes().all(|b| b.is_ascii_digit()) { s.parse().ok() } else { None }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(texte: &str) -> Date {
        texte.parse().expect("date de test")
    }

    #[test]
    fn dates_valides() {
        assert_eq!(date("2026-11-12").to_string(), "2026-11-12");
        assert_eq!(date("12/11/2026").to_string(), "2026-11-12");
        assert_eq!(date(" 1/2/2026 ").to_string(), "2026-02-01");
        assert_eq!(date("2028-02-29").to_string(), "2028-02-29");
        assert_eq!(date("2000-02-29").to_string(), "2000-02-29");
        assert_eq!(date("1985-06-30").to_string(), "1985-06-30");
    }

    #[test]
    fn dates_invalides() {
        for texte in [
            "",
            "2026-02-29",
            "2100-02-29",
            "2026-13-01",
            "2026-04-31",
            "2026-00-10",
            "2026-01-00",
            "26-11-12",
            "2026/11/12",
            "12-11-2026",
            "2026-11-12-01",
            "２０２６-11-12",
            "+202-11-12",
            "1899-12-31",
            "2026-1x-01",
        ] {
            assert!(texte.parse::<Date>().is_err(), "{texte}");
        }
    }

    #[test]
    fn jours_depuis_1970() {
        assert_eq!(date("1970-01-01").jours_epoch(), 0);
        assert_eq!(date("2000-03-01").jours_epoch(), 11_017);
        assert_eq!(date("2026-11-03").jours_epoch(), 20_760);
        for texte in ["1900-01-01", "1999-12-31", "2024-02-29", "2026-11-03", "2100-12-31"] {
            let d = date(texte);
            assert_eq!(Date::depuis_jours_epoch(d.jours_epoch()), Ok(d), "{texte}");
        }
        assert_eq!(date("2026-11-03").plus_jours(30).map(|d| d.to_string()).as_deref(), Ok("2026-12-03"));
        assert_eq!(date("2026-03-01").plus_jours(-1).map(|d| d.to_string()).as_deref(), Ok("2026-02-28"));
        assert_eq!(date("2026-11-03").jours_jusqua(date("2026-12-03")), 30);
        assert!(Date::depuis_jours_epoch(-30_000).is_err());
        let aujourdhui = Date::aujourdhui();
        assert!(aujourdhui.annee() >= 2026);
    }

    #[test]
    fn ordre_chronologique() {
        assert!(date("2026-11-12") < date("2026-11-13"));
        assert!(date("2026-12-01") > date("2026-11-30"));
        assert!(date("2027-01-01") > date("2026-12-31"));
    }

    #[test]
    fn jours_de_la_semaine() {
        assert_eq!(date("2026-11-03").jour_semaine(), "mardi");
        assert_eq!(date("2026-03-01").jour_semaine(), "dimanche");
        assert_eq!(date("2028-02-29").jour_semaine(), "mardi");
        assert_eq!(date("2000-01-01").jour_semaine(), "samedi");
    }

    #[test]
    fn annees_pleines() {
        assert_eq!(date("2000-06-15").annees_pleines_jusqua(date("2026-06-14")), 25);
        assert_eq!(date("2000-06-15").annees_pleines_jusqua(date("2026-06-15")), 26);
        assert_eq!(date("2026-06-15").annees_pleines_jusqua(date("2026-01-01")), -1);
    }

    #[test]
    fn dates_heures() {
        let attendu = "2026-12-14 14:30:00";
        for texte in ["2026-12-14 14:30", "14/12/2026 14h30", "2026-12-14T14:30", "2026-12-14 14:30:00"] {
            assert_eq!(texte.parse::<DateHeure>().map(|d| d.to_string()).as_deref(), Ok(attendu), "{texte}");
        }
        assert_eq!("14/12/2026 9h".parse::<DateHeure>().map(|d| d.to_string()).as_deref(), Ok("2026-12-14 09:00:00"));
        for texte in ["2026-12-14", "2026-12-14 24:00", "2026-12-14 14:60", "2026-12-14 14:30:15", "2026-12-14 midi"] {
            assert!(texte.parse::<DateHeure>().is_err(), "{texte}");
        }
        let neuf: DateHeure = "2026-12-14 09:00".parse().expect("date");
        let dix: DateHeure = "2026-12-14 10:00".parse().expect("date");
        assert!(neuf < dix);
    }
}
