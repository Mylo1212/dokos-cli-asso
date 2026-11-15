//! Comparaison de textes sans casse, accents ni ponctuation.

/// Minuscules, sans accents, ponctuation remplacée par des espaces compactés.
pub fn normaliser(texte: &str) -> String {
    let mut sortie = String::with_capacity(texte.len());
    for c in texte.chars().flat_map(char::to_lowercase) {
        match c {
            'à' | 'á' | 'â' | 'ä' | 'ã' | 'å' => sortie.push('a'),
            'ç' => sortie.push('c'),
            'è' | 'é' | 'ê' | 'ë' => sortie.push('e'),
            'ì' | 'í' | 'î' | 'ï' => sortie.push('i'),
            'ñ' => sortie.push('n'),
            'ò' | 'ó' | 'ô' | 'ö' | 'õ' => sortie.push('o'),
            'ù' | 'ú' | 'û' | 'ü' => sortie.push('u'),
            'ý' | 'ÿ' => sortie.push('y'),
            'œ' => sortie.push_str("oe"),
            'æ' => sortie.push_str("ae"),
            c if c.is_alphanumeric() => sortie.push(c),
            _ => sortie.push(' '),
        }
    }
    sortie.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Vrai si `aiguille` apparaît dans `botte` en mots entiers. Les deux sont
/// supposés déjà normalisés.
pub fn contient_mots(botte: &str, aiguille: &str) -> bool {
    !aiguille.is_empty() && format!(" {botte} ").contains(&format!(" {aiguille} "))
}

/// Nombre d'occurrences en mots entiers (textes normalisés).
pub fn compter_mots(botte: &str, aiguille: &str) -> usize {
    if aiguille.is_empty() {
        return 0;
    }
    format!(" {botte} ").matches(&format!(" {aiguille} ")).count()
}

/// Retire les balises HTML et décode les entités courantes.
pub fn texte_visible(html: &str) -> String {
    let mut sortie = String::with_capacity(html.len());
    let mut dans_balise = false;
    for c in html.chars() {
        match c {
            '<' => dans_balise = true,
            '>' if dans_balise => {
                dans_balise = false;
                sortie.push(' ');
            }
            _ if !dans_balise => sortie.push(c),
            _ => {}
        }
    }
    const ENTITES: &[(&str, &str)] = &[
        ("&nbsp;", " "),
        ("&lt;", "<"),
        ("&gt;", ">"),
        ("&quot;", "\""),
        ("&#39;", "'"),
        ("&rsquo;", "'"),
        ("&eacute;", "é"),
        ("&egrave;", "è"),
        ("&ecirc;", "ê"),
        ("&agrave;", "à"),
        ("&acirc;", "â"),
        ("&ccedil;", "ç"),
        ("&ocirc;", "ô"),
        ("&ucirc;", "û"),
        ("&icirc;", "î"),
        ("&amp;", "&"),
    ];
    ENTITES.iter().fold(sortie, |texte, (entite, caractere)| texte.replace(entite, caractere))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalisation() {
        assert_eq!(normaliser("Période d’essai"), "periode d essai");
        assert_eq!(normaliser("CONGÉS  payés."), "conges payes");
        assert_eq!(normaliser("Œuvre"), "oeuvre");
    }

    #[test]
    fn mots_entiers() {
        let texte = normaliser("Les heures complémentaires sont limitées");
        assert!(contient_mots(&texte, &normaliser("heures complémentaires")));
        assert!(!contient_mots(&texte, "heure"));
        assert!(!contient_mots(&texte, ""));
        assert_eq!(compter_mots("a completer x a completer", "a completer"), 2);
    }

    #[test]
    fn html_visible() {
        let texte = texte_visible("<p>Convention <b>collective</b>&nbsp;: p&eacute;riode</p>");
        assert_eq!(normaliser(&texte), "convention collective periode");
    }
}
