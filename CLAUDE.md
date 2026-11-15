# Consignes pour ce dépôt

## Répartition des modèles Claude

Réglée dans `.claude/settings.json` et `.claude/agents/` :

- **Réflexion et plan** : Opus, effort élevé (modèle `opusplan`, en mode plan).
- **Exécution** : Sonnet, effort élevé (bascule automatique d'`opusplan` une fois le
  plan validé). Pour les tâches non triviales, commencer donc en mode plan.
- **Recherche documentaire** : déléguer à l'agent `documentaliste` (Haiku, lecture
  seule) toute vérification d'API, d'option ou de version.

## Tests et vérifications : seulement sur demande

- Ne lancer ni les tests (`cargo test`, `npm test`, `pytest`…) ni les vérifications
  (`cargo fmt --check`, `cargo clippy`, `npx tsc --noEmit`, linters…) systématiquement :
  uniquement quand l'utilisateur le demande.
- Après une modification, dire en une phrase s'il est pertinent de tester ou de vérifier
  et pourquoi (code touché, risque de régression, ce que la CI vérifiera de toute façon),
  puis attendre sa décision.
- La CI GitHub lance tests et vérifications à chaque push.
