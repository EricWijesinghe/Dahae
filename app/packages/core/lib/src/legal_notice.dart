import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'i18n.dart';

/// The Dahae license text, copied from the repository's LICENSE (a CI check keeps the two equal).
/// Apps load it as `packages/core/...`; inside this package its own bundle key has no prefix.
const dahaeLicenseAssetKey = 'packages/core/assets/LICENSE-DAHAE.txt';
var _dahaeLicenseRegistered = false;

/// Adds the Dahae license to the licenses page once per run; repeated calls do nothing.
void registerDahaeLicense({String assetKey = dahaeLicenseAssetKey}) {
  if (_dahaeLicenseRegistered) return;
  _dahaeLicenseRegistered = true;
  LicenseRegistry.addLicense(() async* {
    final text = await rootBundle.loadString(assetKey);
    yield LicenseEntryWithLineBreaks(const ['Dahae'], text);
  });
}

/// The open-source licenses page: owner copyright line, the Dahae license, then third-party entries.
void showDahaeLicensePage(
  BuildContext context, {
  required String applicationName,
  String? applicationVersion,
}) {
  registerDahaeLicense();
  showLicensePage(
    context: context,
    applicationName: applicationName,
    applicationVersion: applicationVersion,
    applicationLegalese: context.t.legalCopyright,
  );
}
