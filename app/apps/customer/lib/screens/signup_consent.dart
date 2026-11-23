// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:ui/ui.dart';

/// Policy version the signup consents name. Must match backend privacy::CONSENT_VERSION: the server
/// refuses any other version, so bump both together.
const String signupConsentVersion = '2026-11-17';

/// Kinds asked at signup, in display order. The first three are required to open an account.
const List<String> signupConsentKinds = [
  'terms',
  'privacy',
  'overseas_transfer',
  'marketing',
];
const List<String> signupRequiredKinds = [
  'terms',
  'privacy',
  'overseas_transfer',
];

/// Nothing ticked yet.
Map<String, bool> emptySignupConsents() => {
  for (final k in signupConsentKinds) k: false,
};

bool signupRequiredAgreed(Map<String, bool> granted) =>
    signupRequiredKinds.every((k) => granted[k] == true);

/// Body for /auth/verify: every kind with its answer and the policy version the text was shown under.
List<Json> signupConsentPayload(Map<String, bool> granted) => [
  for (final k in signupConsentKinds)
    {
      'kind': k,
      'granted': granted[k] ?? false,
      'version': signupConsentVersion,
    },
];

/// Checkboxes on the signup screen: three required items and one optional marketing item.
class SignupConsentChecks extends StatelessWidget {
  const SignupConsentChecks({
    super.key,
    required this.granted,
    required this.onChanged,
  });

  final Map<String, bool> granted;
  final void Function(String kind, bool value) onChanged;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    return Column(
      children: [
        for (final kind in signupConsentKinds)
          CheckboxListTile(
            key: ValueKey('signup-consent-$kind'),
            value: granted[kind] ?? false,
            controlAffinity: ListTileControlAffinity.leading,
            title: KeepAllText(_title(t, kind)),
            subtitle: _subtitle(context, t, kind),
            secondary: KeepAllText(
              signupRequiredKinds.contains(kind) ? t.required : t.pvOptional,
              style: context.text.labelSmall,
            ),
            onChanged: (v) => onChanged(kind, v ?? false),
          ),
      ],
    );
  }

  static String _title(T t, String kind) => switch (kind) {
    'terms' => t.pvTerms,
    'privacy' => t.pvPrivacy,
    'overseas_transfer' => t.pvOverseas,
    _ => t.pvMarketing,
  };

  static Widget? _subtitle(BuildContext context, T t, String kind) {
    return switch (kind) {
      'overseas_transfer' => KeepAllText(
        t.sgOverseasNote,
        style: context.text.bodySmall,
      ),
      'marketing' => KeepAllText(t.pvMarketingHint, style: context.text.bodySmall),
      _ => null,
    };
  }
}
