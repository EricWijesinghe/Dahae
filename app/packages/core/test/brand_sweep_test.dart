// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

/// A string literal that names the built-in brand.
final _brandLiteral = RegExp(r'''(['"])[^'"\n]*(다해|Dahae)[^'"\n]*\1''');

/// Where the built-in name may stay: the fallback brand, the font family and the licensor's licence entry.
const _allowed = {
  'packages/ui/lib/src/tokens.dart': "'다해'",
  'packages/core/lib/src/legal_notice.dart': "'Dahae'",
};

/// ARB keys that may name the built-in brand: the licensor's copyright line.
const _allowedKeys = {'legalCopyright'};

void main() {
  test('the apps take the business name from the profile, never a literal', () {
    final hits = <String>[];
    // Paths from this package (the test's working directory) and how the allow list names them.
    for (final (dir, label) in [
      ('lib', 'packages/core/lib'),
      ('../ui/lib', 'packages/ui/lib'),
      ('../../apps/customer/lib', 'apps/customer/lib'),
      ('../../apps/partner/lib', 'apps/partner/lib'),
    ]) {
      for (final f in Directory(dir).listSync(recursive: true)) {
        final path = f.path.replaceAll(r'\', '/');
        if (f is! File || !path.endsWith('.dart') || path.contains('/gen/')) {
          continue;
        }
        final key = '$label${path.substring(dir.length)}';
        final lines = f.readAsLinesSync();
        for (var i = 0; i < lines.length; i++) {
          final line = lines[i].trim();
          if (line.startsWith('//')) continue;
          for (final m in _brandLiteral.allMatches(line)) {
            final s = m.group(0)!;
            if (s.contains('Dahae Sans') || _allowed[key] == s) continue;
            hits.add('$key:${i + 1}: $s');
          }
        }
      }
    }
    expect(hits, isEmpty);
  });

  test('translated texts use {brand} instead of the built-in name', () {
    for (final lang in ['ko', 'en']) {
      final arb =
          jsonDecode(File('lib/l10n/app_$lang.arb').readAsStringSync())
              as Map<String, dynamic>;
      final named = [
        for (final e in arb.entries)
          if (!e.key.startsWith('@') &&
              !_allowedKeys.contains(e.key) &&
              e.value is String &&
              RegExp('다해|Dahae').hasMatch(e.value as String))
            e.key,
      ];
      expect(named, isEmpty, reason: lang);
    }
  });
}
