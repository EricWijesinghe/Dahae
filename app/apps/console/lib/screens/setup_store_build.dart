// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:intl/intl.dart';
import 'package:ui/ui.dart';
import 'package:url_launcher/url_launcher.dart';

import '../state.dart';

/// What needs a new store build: the brand values store apps bake in that changed since the last
/// recorded build (`store_build` in `/admin/setup`), the build page, and [빌드했어요] (owner) to record it.
class StoreBuildCard extends StatefulWidget {
  const StoreBuildCard({super.key, required this.data, required this.onAcked});
  final Json data;
  final VoidCallback onAcked;

  @override
  State<StoreBuildCard> createState() => _StoreBuildCardState();
}

class _StoreBuildCardState extends State<StoreBuildCard> {
  late final _url = TextEditingController(
    text: widget.data['build_url'] as String? ?? '',
  );
  bool _busy = false;

  @override
  void dispose() {
    _url.dispose();
    super.dispose();
  }

  static String _item(T t, Object? id) => switch (id) {
    'name' => t.cRbStoreItemName,
    'icon' => t.cRbStoreItemIcon,
    'color' => t.cRbStoreItemColor,
    'app_ids' => t.cRbStoreItemAppIds,
    'domain' => t.cRbStoreItemDomain,
    _ => '$id',
  };

  Future<void> _ack() async {
    final t = context.t, url = _url.text.trim();
    setState(() => _busy = true);
    try {
      await context.api.post('/admin/setup/store-build/ack', {
        if (url.isNotEmpty) 'build_url': url,
      });
      if (!mounted) return;
      showToast(context, t.cRbStoreAcked);
      widget.onAcked();
    } catch (e) {
      if (mounted) showToast(context, t.error(e));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final changed = (widget.data['changed'] as List? ?? const []);
    final acked = DateTime.tryParse(widget.data['acked_at'] as String? ?? '');
    final url = Uri.tryParse(_url.text.trim());
    return AppCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Icon(
                changed.isEmpty
                    ? Icons.check_circle_rounded
                    : Icons.system_update_rounded,
                color: changed.isEmpty ? context.primary : context.colors.sun,
              ),
              const SizedBox(width: Space.sm),
              Expanded(
                child: KeepAllText(t.cRbStoreTitle, style: context.text.titleSmall),
              ),
            ],
          ),
          const SizedBox(height: Space.xs),
          KeepAllText(t.cRbStoreBody, style: context.text.bodySmall),
          const SizedBox(height: Space.sm),
          KeepAllText(
            changed.isEmpty
                ? t.cRbStoreNone
                : t.cRbStoreChanged(changed.map((c) => _item(t, c)).join(', ')),
            style: context.text.bodyMedium,
          ),
          if (acked != null)
            KeepAllText(
              t.cRbStoreLastAck(
                DateFormat('yyyy-MM-dd HH:mm').format(acked.toLocal()),
              ),
              style: context.text.bodySmall,
            ),
          const SizedBox(height: Space.sm),
          TextField(
            controller: _url,
            keyboardType: TextInputType.url,
            decoration: InputDecoration(labelText: t.cRbStoreBuildUrl),
            onChanged: (_) => setState(() {}),
          ),
          const SizedBox(height: Space.sm),
          Wrap(
            spacing: Space.sm,
            runSpacing: Space.sm,
            children: [
              OutlinedButton.icon(
                onPressed: url != null && url.scheme == 'https'
                    ? () => launchUrl(url, mode: LaunchMode.externalApplication)
                    : null,
                icon: const Icon(Icons.open_in_new_rounded, size: 18),
                label: KeepAllText(t.cRbStoreOpen),
              ),
              OutlinedButton(
                onPressed: _busy ? null : _ack,
                child: KeepAllText(t.cRbStoreAck),
              ),
            ],
          ),
          ExpansionTile(
            tilePadding: EdgeInsets.zero,
            title: KeepAllText(
              t.cRbStoreBuildTimeTitle,
              style: context.text.labelLarge,
            ),
            expandedAlignment: Alignment.centerLeft,
            children: [
              KeepAllText(t.cRbStoreBuildTimeList, style: context.text.bodySmall),
            ],
          ),
          ExpansionTile(
            tilePadding: EdgeInsets.zero,
            title: KeepAllText(t.cRbStoreOutsideTitle, style: context.text.labelLarge),
            expandedAlignment: Alignment.centerLeft,
            children: [
              KeepAllText(t.cRbStoreOutsideList, style: context.text.bodySmall),
            ],
          ),
        ],
      ),
    );
  }
}
