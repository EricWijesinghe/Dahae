import 'package:core/core.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  test('the owner copyright line matches NOTICE in both languages', () async {
    final ko = await T.delegate.load(const Locale('ko'));
    final en = await T.delegate.load(const Locale('en'));
    expect(
      ko.legalCopyright,
      '다해 소프트웨어 © 2026 Eric Wijesinghe. All rights reserved.',
    );
    expect(
      en.legalCopyright,
      'Dahae software © 2026 Eric Wijesinghe. All rights reserved.',
    );
  });

  test(
    'the Dahae license is registered once, under the package name Dahae',
    () async {
      registerDahaeLicense(assetKey: 'assets/LICENSE-DAHAE.txt');
      registerDahaeLicense(assetKey: 'assets/LICENSE-DAHAE.txt');
      final entries = await LicenseRegistry.licenses.toList();
      final dahae = entries.whereType<LicenseEntryWithLineBreaks>().where(
        (e) => e.packages.contains('Dahae'),
      );
      expect(dahae, hasLength(1));
      expect(
        dahae.single.paragraphs.map((p) => p.text).join('\n'),
        contains('Dahae Proprietary Software License'),
      );
    },
  );
}
