/* External scanner for tree-sitter-lute.
 *
 * FRONTMATTER: a leading YAML `---` … `---` envelope (dsl §6.1). This cannot
 * be a plain `token(prec(...))` regex because the block runs
 * delimiter-to-delimiter and a *body* line may itself look like a delimiter,
 * so the boundary is decided line-by-line, not by a (greedy) DFA. The body is
 * opaque (parsed as YAML by the checker, §6.1); we only find where it ends.
 * The token spans from the opening `---\n` through the closing `---` line's
 * newline (or EOF). It is only ever valid at the document start, so the scanner
 * simply reports "no frontmatter" whenever the first non-trivia char is not `-`.
 *
 * MODIFIER_NAME: the `:name` head of an inline text modifier (dsl 0.37.0
 * §3.6), `name` a lowerCamelName. It is a modifier only when the very next
 * char is `[` or `{`; otherwise the `:` is ordinary text. That one-char
 * lookahead past the token's end is why this is external. Immediate: never
 * skips whitespace.
 */

#include "tree_sitter/parser.h"

enum TokenType {
  FRONTMATTER,
  MODIFIER_NAME,
};

static bool is_lower(int32_t c) { return c >= 'a' && c <= 'z'; }

static bool is_name_char(int32_t c) {
  return is_lower(c) || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9');
}

/* `:name` followed (not consumed) by `[` or `{`. */
static bool scan_modifier_name(TSLexer *lexer) {
  if (lexer->lookahead != ':') {
    return false;
  }
  lexer->advance(lexer, false);
  if (!is_lower(lexer->lookahead)) {
    return false;
  }
  while (is_name_char(lexer->lookahead)) {
    lexer->advance(lexer, false);
  }
  lexer->mark_end(lexer);
  if (lexer->lookahead != '[' && lexer->lookahead != '{') {
    return false;
  }
  lexer->result_symbol = MODIFIER_NAME;
  return true;
}

/* Consume the rest of the current line, including its `\n` (or up to EOF). */
static void skip_to_eol(TSLexer *lexer) {
  while (!lexer->eof(lexer) && lexer->lookahead != '\n') {
    lexer->advance(lexer, false);
  }
  if (lexer->lookahead == '\n') {
    lexer->advance(lexer, false); /* consume the newline */
  }
}

/* Count leading '-' at the cursor, advancing past them. */
static unsigned count_dashes(TSLexer *lexer) {
  unsigned n = 0;
  while (lexer->lookahead == '-') {
    lexer->advance(lexer, false);
    n++;
  }
  return n;
}

/* True if, after `count_dashes`, the line is exactly `---` (EOL or EOF next). */
static bool is_delimiter_tail(TSLexer *lexer) {
  return lexer->eof(lexer) || lexer->lookahead == '\n' || lexer->lookahead == '\r';
}

bool tree_sitter_lute_external_scanner_scan(void *payload, TSLexer *lexer,
                                            const bool *valid_symbols) {
  (void)payload;
  /* Both valid at once only during error recovery (FRONTMATTER is legal solely
   * at the document start, MODIFIER_NAME solely mid-text): claim nothing then,
   * so recovery never mistakes a `::directive{` tail for a modifier. */
  if (valid_symbols[FRONTMATTER] && valid_symbols[MODIFIER_NAME]) {
    return false;
  }
  if (valid_symbols[MODIFIER_NAME]) {
    return scan_modifier_name(lexer);
  }
  if (!valid_symbols[FRONTMATTER]) {
    return false;
  }

  /* Opening delimiter must be exactly `---` at the very start. */
  if (lexer->lookahead != '-') {
    return false;
  }
  if (count_dashes(lexer) != 3 || !is_delimiter_tail(lexer)) {
    return false;
  }
  /* Consume the opener's line terminator; if none, this isn't a block. */
  if (lexer->lookahead == '\r') {
    lexer->advance(lexer, false);
  }
  if (lexer->lookahead != '\n') {
    return false;
  }
  lexer->advance(lexer, false);

  /* Scan body lines until a closing `---` line or EOF. */
  for (;;) {
    if (lexer->eof(lexer)) {
      /* Unterminated frontmatter: don't consume as a token. */
      return false;
    }
    if (lexer->lookahead == '-') {
      unsigned dashes = count_dashes(lexer);
      if (dashes == 3 && is_delimiter_tail(lexer)) {
        /* Closing delimiter: consume its line terminator, then finish. */
        if (lexer->lookahead == '\r') {
          lexer->advance(lexer, false);
        }
        if (lexer->lookahead == '\n') {
          lexer->advance(lexer, false);
        }
        lexer->mark_end(lexer);
        lexer->result_symbol = FRONTMATTER;
        return true;
      }
      /* A `-`-led body line that isn't a closer: eat the remainder. */
      skip_to_eol(lexer);
    } else {
      skip_to_eol(lexer);
    }
  }
}

/* Stateless scanner: nothing to (de)serialize. */
void *tree_sitter_lute_external_scanner_create(void) { return NULL; }
void tree_sitter_lute_external_scanner_destroy(void *payload) { (void)payload; }
unsigned tree_sitter_lute_external_scanner_serialize(void *payload, char *buffer) {
  (void)payload;
  (void)buffer;
  return 0;
}
void tree_sitter_lute_external_scanner_deserialize(void *payload, const char *buffer,
                                                   unsigned length) {
  (void)payload;
  (void)buffer;
  (void)length;
}
