// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:flutter/foundation.dart' show kIsWeb;
import 'package:flutter/material.dart';

import 'api.dart';
import 'desktop.dart';
import 'portone_webview.dart';

/// Server's answer for saving a card on the gateway that saves cards (PortOne first, else Toss Payments): PortOne
/// store and per-method channel keys, or the Toss billing client key under `card`. Disabled while none can.
class BillingSetup {
  const BillingSetup({
    required this.enabled,
    this.gateway = 'portone',
    this.storeId = '',
    this.channels = const {},
    this.issueId = '',
    this.customerId = '',
  });
  final bool enabled;

  /// `portone` or `tosspayments`: picks the registration page.
  final String gateway;
  final String storeId;
  final Map<String, String> channels;

  /// PortOne issueId, or the Toss customerKey the card is registered under.
  final String issueId;

  /// PortOne `customer.customerId`: the server checks the issued key belongs to this customer.
  final String customerId;

  static const off = BillingSetup(enabled: false);

  bool get isToss => gateway == 'tosspayments';

  factory BillingSetup.fromJson(Map<String, dynamic> j) {
    if (j['enabled'] != true) return off;
    return BillingSetup(
      enabled: true,
      gateway: j['gateway'] as String? ?? 'portone',
      storeId: j['store_id'] as String? ?? '',
      channels: {
        for (final e in (j['channels'] as Map? ?? {}).entries)
          e.key as String: e.value as String,
      },
      issueId: j['issue_id'] as String? ?? '',
      customerId: j['customer_id'] as String? ?? '',
    );
  }

  /// Whether [provider] can be saved for one-tap payment right now.
  /// Card registration runs in a WebView, which the web and Windows builds lack, so saved cards are phone-only.
  bool supports(String provider) =>
      !kIsWeb && !isWindowsApp() && enabled && channels.containsKey(provider);
}

/// A failed read means the option stays hidden.
Future<BillingSetup> fetchBillingSetup(ApiClient api) async {
  try {
    return BillingSetup.fromJson(
      await api.get('/me/payment-methods/setup') as Map<String, dynamic>,
    );
  } on Object {
    return BillingSetup.off;
  }
}

/// The hosted card registration page for [setup]'s gateway: website/pay/billing.html (PortOne) or
/// website/pay/toss-billing.html (Toss, where the issue id is the customerKey).
Uri billingPageUri(String apiBase, BillingSetup setup, String provider) =>
    Uri.parse(
      setup.isToss
          ? '$apiBase/pay/toss-billing.html'
          : '$apiBase/pay/billing.html',
    ).replace(
      queryParameters: {
        if (!setup.isToss) 'store': setup.storeId,
        'channel': setup.channels[provider] ?? '',
        'issue': setup.issueId,
        if (!setup.isToss) 'customer': setup.customerId,
        'method': provider,
        'app': payAppScheme,
      },
    );

/// Opens the gateway's card registration (hosted page in a WebView) and returns what the server needs to
/// save the card: the PortOne billing key or the Toss authKey.
/// Null when the customer backs out or registration fails; the server verifies the key on save.
Future<String?> issueBillingKey({
  required BuildContext context,
  required String apiBase,
  required BillingSetup setup,
  required String provider,
  required String pageTitle,
}) {
  final uri = billingPageUri(apiBase, setup, provider);
  return Navigator.of(context).push<String?>(
    MaterialPageRoute(
      fullscreenDialog: true,
      builder: (_) => HostedPage<String?>(
        uri: uri,
        donePath: billingDonePath,
        title: pageTitle,
        onDone: billingKeyFromRedirect,
      ),
    ),
  );
}

/// Path the hosted billing page redirects to; the WebView intercepts it.
const billingDonePath = '/pay/billing-done.html';

/// Reads PortOne's `billingKey` or Toss's `authKey` from the redirect. Missing or empty (cancel, error) reads as null.
String? billingKeyFromRedirect(Uri uri) {
  final key =
      uri.queryParameters['billingKey'] ?? uri.queryParameters['authKey'];
  return key == null || key.isEmpty ? null : key;
}
