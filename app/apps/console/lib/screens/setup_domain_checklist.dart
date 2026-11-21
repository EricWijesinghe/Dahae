// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:ui/ui.dart';

import '../state.dart';
import '../widgets/copy_row.dart';
import '../widgets/owner_totp_dialog.dart';

/// Label for a post-move checklist id (the ids the server sends, `domains.rs`). Unknown ids show as is.
String checklistLabel(T t, String id) => switch (id) {
  'portone_webhook' => t.cRbCheck_portone_webhook,
  'toss_payments_webhook' => t.cRbCheck_toss_payments_webhook,
  'toss_payouts_webhook' => t.cRbCheck_toss_payouts_webhook,
  'pg_site_url' => t.cRbCheck_pg_site_url,
  'naver_callback' => t.cRbCheck_naver_callback,
  'naver_service_url' => t.cRbCheck_naver_service_url,
  'kakao_site_domain' => t.cRbCheck_kakao_site_domain,
  'google_consent' => t.cRbCheck_google_consent,
  'search_console' => t.cRbCheck_search_console,
  'naver_search_advisor' => t.cRbCheck_naver_search_advisor,
  'apple_notifications' => t.cRbCheck_apple_notifications,
  'app_store_urls' => t.cRbCheck_app_store_urls,
  'play_urls' => t.cRbCheck_play_urls,
  'ms_store_urls' => t.cRbCheck_ms_store_urls,
  'github_store_vars' => t.cRbCheck_github_store_vars,
  'alimtalk_links' => t.cRbCheck_alimtalk_links,
  'dns_keep_old' => t.cRbCheck_dns_keep_old,
  'version_floor' => t.cRbCheck_version_floor,
  _ => id,
};

/// The outside services to update after a move, each with the exact value to paste and a tick.
class DomainChecklist extends StatefulWidget {
  const DomainChecklist({super.key, required this.items});
  final List<Json> items;

  @override
  State<DomainChecklist> createState() => _DomainChecklistState();
}

class _DomainChecklistState extends State<DomainChecklist> {
  late final Map<String, bool> _done = {
    for (final i in widget.items) i['id'] as String: i['done'] == true,
  };

  Future<void> _tick(String id, bool v) async {
    setState(() => _done[id] = v);
    try {
      await context.api.put('/admin/domains/checklist/$id', {'done': v});
    } catch (e) {
      if (!mounted) return;
      setState(() => _done[id] = !v);
      showToast(context, context.t.error(e));
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(t.cRbChecklistTitle, style: context.text.labelLarge),
        Text(t.cRbChecklistHint, style: context.text.bodySmall),
        for (final i in widget.items)
          Row(
            children: [
              Checkbox(
                value: _done[i['id']] ?? false,
                onChanged: (v) => _tick(i['id'] as String, v ?? false),
              ),
              Expanded(child: _item(t, i)),
            ],
          ),
      ],
    );
  }

  Widget _item(T t, Json i) {
    final label = checklistLabel(t, i['id'] as String);
    final value = i['value'] as String? ?? '';
    return value.isEmpty
        ? Padding(
            padding: const EdgeInsets.symmetric(vertical: Space.sm),
            child: Text(label, style: context.text.labelLarge),
          )
        : _values(label, value.split('\n'));
  }

  /// One copy button per line, so each paste fills one field. A `KEY=value` line is a variable: the key is
  /// its label and only the value is copied. A URL line is labelled by its page name.
  Widget _values(String label, List<String> lines) {
    if (lines.length == 1) return CopyRow(label: label, value: lines.first);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Padding(
          padding: const EdgeInsets.only(top: Space.sm),
          child: Text(label, style: context.text.labelLarge),
        ),
        for (final l in lines)
          if (_variable.firstMatch(l) case final m?)
            CopyRow(label: m[1]!, value: m[2]!)
          else
            CopyRow(
              label: Uri.tryParse(l)?.pathSegments.lastOrNull ?? '',
              value: l,
            ),
      ],
    );
  }
}

final _variable = RegExp(r'^([A-Z][A-Z0-9_]*)=(.*)$');

/// An old address that still answers, with [닫기]: owner code, after the owner confirms the store builds
/// with the new address are out (the dialog shows the current minimum app versions).
class AliasRow extends StatelessWidget {
  const AliasRow({
    super.key,
    required this.host,
    required this.business,
    required this.onRetired,
  });
  final String host;
  final Json business;
  final Future<void> Function() onRetired;

  String _floor(String app) {
    final v = (business['app_versions'] as Json?)?[app] as Json?;
    final s = v?['min_app_version'] as String? ?? '';
    return s.isEmpty ? '-' : s;
  }

  Future<void> _retire(BuildContext context) async {
    final t = context.t, api = context.api;
    final sure = await showDialog<bool>(
      context: context,
      builder: (_) => _RetireDialog(
        host: host,
        body: t.cRbRetireBody(_floor('customer'), _floor('partner')),
      ),
    );
    if (sure != true || !context.mounted) return;
    final ok = await runOwnerAction(context, (h) async {
      await api.delete('/admin/domains/${Uri.encodeComponent(host)}', null, h);
      return true;
    }, done: t.cRbRetired);
    if (ok == true) await onRetired();
  }

  @override
  Widget build(BuildContext context) => Row(
    children: [
      Expanded(child: Text(host, style: context.text.bodyMedium)),
      TextButton(
        onPressed: () => _retire(context),
        child: Text(context.t.cRbDomainRetire),
      ),
    ],
  );
}

class _RetireDialog extends StatefulWidget {
  const _RetireDialog({required this.host, required this.body});
  final String host;
  final String body;

  @override
  State<_RetireDialog> createState() => _RetireDialogState();
}

class _RetireDialogState extends State<_RetireDialog> {
  bool _confirmed = false;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    return AlertDialog(
      title: Text(t.cRbRetireTitle(widget.host)),
      content: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(widget.body),
          CheckboxListTile(
            contentPadding: EdgeInsets.zero,
            value: _confirmed,
            onChanged: (v) => setState(() => _confirmed = v ?? false),
            title: Text(t.cRbRetireConfirm),
          ),
        ],
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(false),
          child: Text(t.cancel),
        ),
        FilledButton(
          onPressed: _confirmed ? () => Navigator.of(context).pop(true) : null,
          child: Text(t.cRbDomainRetire),
        ),
      ],
    );
  }
}
