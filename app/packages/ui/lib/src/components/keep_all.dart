// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:flutter/widgets.dart';

const _wordJoiner = '\u2060';

/// Text with at least two words, so there is a space to break at instead.
final _multiWord = RegExp(r'\S\s+\S');

/// A spot between a Hangul syllable (or compatibility jamo) and a neighbouring non-space character. Zero-width space
/// (U+200B) and word joiner count as spaces: the first is an explicit break hint, the second is already there.
final _joinable = RegExp(
  '(?<=[\\uAC00-\\uD7A3\\u3131-\\u318E])(?=[^\\s\\u200B\\u2060])|'
  '(?<=[^\\s\\u200B\\u2060])(?=[\\uAC00-\\uD7A3\\u3131-\\u318E])',
);

/// [text] with Korean words held together, like CSS `word-break: keep-all`: lines break only at spaces.
///
/// Flutter breaks Hangul between any two syllables ("그대/로"); a word joiner (U+2060) between the characters of
/// each Korean word forbids that. Text without Hangul, and single-word text (it has nowhere else to break, so it
/// still breaks per syllable when it must), comes back unchanged. Display only: the joiners are invisible but are
/// part of the string, so never store, compare or copy the result.
String keepAll(String text) =>
    _multiWord.hasMatch(text) ? text.replaceAll(_joinable, _wordJoiner) : text;

/// A [Text] that wraps Korean only at spaces ([keepAll]), for every string the apps show.
///
/// [data] stays the plain string, so finders, semantics and screen readers see the normal text. Inside a
/// [SelectionArea] it draws [data] as is, so copied text carries no hidden joiners.
class KeepAllText extends Text {
  const KeepAllText(
    super.data, {
    super.key,
    super.style,
    super.strutStyle,
    super.textAlign,
    super.textDirection,
    super.locale,
    super.softWrap,
    super.overflow,
    super.textScaler,
    super.maxLines,
    super.semanticsLabel,
    super.semanticsIdentifier,
    super.textWidthBasis,
    super.textHeightBehavior,
    super.selectionColor,
  });

  @override
  Widget build(BuildContext context) {
    final plain = data!;
    final shown = keepAll(plain);
    // One line never wraps, so there is nothing to keep together; joiners there would only move the ellipsis.
    final wraps = maxLines != 1 && softWrap != false;
    if (!wraps ||
        shown == plain ||
        SelectionContainer.maybeOf(context) != null) {
      return super.build(context);
    }
    // Text.build only reads this context, so the joined copy renders as this widget, with no extra element.
    return Text.rich(
      TextSpan(text: shown, semanticsLabel: plain),
      style: style,
      strutStyle: strutStyle,
      textAlign: textAlign,
      textDirection: textDirection,
      locale: locale,
      softWrap: softWrap,
      overflow: overflow,
      textScaler: textScaler,
      maxLines: maxLines,
      semanticsLabel: semanticsLabel,
      semanticsIdentifier: semanticsIdentifier,
      textWidthBasis: textWidthBasis,
      textHeightBehavior: textHeightBehavior,
      selectionColor: selectionColor,
    ).build(context);
  }
}
