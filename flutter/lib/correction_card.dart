import 'package:flutter/material.dart';

import 'models.dart';

/// A single correction proposal.
///
/// Shows the plain-language explanation, the exact before/after of the
/// affected line with the changed span highlighted, a confidence badge,
/// and Approve / Reject actions. Also shows a warning icon when the
/// correction may change the document's meaning.
///
/// The card never writes to disk. It reports the user's decision back via
/// `onApprove` and `onReject`.
class CorrectionCard extends StatelessWidget {
  final Correction correction;
  final VoidCallback? onApprove;
  final VoidCallback? onReject;
  final VoidCallback? onNavigate;
  final bool busy;

  const CorrectionCard({
    super.key,
    required this.correction,
    this.onApprove,
    this.onReject,
    this.onNavigate,
    this.busy = false,
  });

  @override
  Widget build(BuildContext context) {
    final c = correction;

    Color badgeColor;
    String badgeLabel;
    switch (c.confidence) {
      case 'Certain':
        badgeColor = Colors.green;
        badgeLabel = 'Certain';
        break;
      case 'Uncertain':
        badgeColor = Colors.orange;
        badgeLabel = 'Uncertain';
        break;
      default:
        badgeColor = Colors.blue;
        badgeLabel = 'Likely';
    }

    return Card(
      margin: const EdgeInsets.fromLTRB(8, 4, 8, 4),
      child: Padding(
        padding: const EdgeInsets.all(12),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Icon(
                  c.isDeletion ? Icons.delete_outline : Icons.build_outlined,
                  size: 18,
                ),
                const SizedBox(width: 6),
                Expanded(
                  child: InkWell(
                    onTap: onNavigate,
                    borderRadius: BorderRadius.circular(4),
                    child: Padding(
                      padding: const EdgeInsets.symmetric(horizontal: 4, vertical: 2),
                      child: Row(
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          Flexible(
                            child: Text(
                              '${c.file}:${c.line}',
                              style: TextStyle(
                                fontFamily: 'monospace',
                                fontSize: 12,
                                color: onNavigate != null
                                    ? Theme.of(context).colorScheme.primary
                                    : null,
                                decoration: onNavigate != null
                                    ? TextDecoration.underline
                                    : null,
                              ),
                              overflow: TextOverflow.ellipsis,
                            ),
                          ),
                          if (onNavigate != null) ...[
                            const SizedBox(width: 4),
                            Icon(
                              Icons.arrow_outward,
                              size: 12,
                              color: Theme.of(context).colorScheme.primary,
                            ),
                          ],
                        ],
                      ),
                    ),
                  ),
                ),
                Container(
                  padding:
                      const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
                  decoration: BoxDecoration(
                    color: badgeColor.withValues(alpha: 0.2),
                    borderRadius: BorderRadius.circular(4),
                  ),
                  child: Text(
                    badgeLabel,
                    style: TextStyle(
                      fontSize: 10,
                      color: badgeColor,
                      fontWeight: FontWeight.bold,
                    ),
                  ),
                ),
                if (c.mayChangeMeaning)
                  const Padding(
                    padding: EdgeInsets.only(left: 4),
                    child: Tooltip(
                      message: 'Applying this may change the meaning of the '
                          'document. Review carefully before approving.',
                      child: Icon(
                        Icons.warning_amber,
                        size: 16,
                        color: Colors.orange,
                      ),
                    ),
                  ),
              ],
            ),
            const SizedBox(height: 6),
            Text(c.explanation, style: const TextStyle(fontSize: 13)),
            const SizedBox(height: 10),
            _DiffLine(
              label: 'before',
              full: c.context,
              highlight: c.oldText,
              colour: Colors.red,
            ),
            const SizedBox(height: 4),
            _DiffLine(
              label: 'after',
              full: c.previewAfter(),
              highlight: c.newText,
              colour: Colors.green,
            ),
            const SizedBox(height: 8),
            Row(
              mainAxisAlignment: MainAxisAlignment.end,
              children: [
                TextButton(
                  onPressed: busy ? null : onReject,
                  child: const Text('Reject'),
                ),
                const SizedBox(width: 8),
                FilledButton(
                  onPressed: busy ? null : onApprove,
                  child: busy
                      ? const SizedBox(
                          width: 16,
                          height: 16,
                          child: CircularProgressIndicator(strokeWidth: 2),
                        )
                      : Text(c.isDeletion ? 'Delete' : 'Apply'),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

/// One line of a diff, with `highlight` rendered in the given colour.
class _DiffLine extends StatelessWidget {
  final String label;
  final String full;
  final String highlight;
  final Color colour;

  const _DiffLine({
    required this.label,
    required this.full,
    required this.highlight,
    required this.colour,
  });

  @override
  Widget build(BuildContext context) {
    final spans = <TextSpan>[];

    if (highlight.isEmpty || !full.contains(highlight)) {
      spans.add(TextSpan(text: full));
    } else {
      final i = full.indexOf(highlight);
      spans.add(TextSpan(text: full.substring(0, i)));
      spans.add(TextSpan(
        text: highlight,
        style: TextStyle(
          backgroundColor: colour.withValues(alpha: 0.35),
          fontWeight: FontWeight.bold,
        ),
      ));
      spans.add(TextSpan(text: full.substring(i + highlight.length)));
    }

    return Container(
      width: double.infinity,
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 6),
      decoration: BoxDecoration(
        color: colour.withValues(alpha: 0.06),
        border: Border(left: BorderSide(color: colour, width: 2)),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            label,
            style: TextStyle(
              fontSize: 10,
              color: colour,
              fontWeight: FontWeight.bold,
              letterSpacing: 0.5,
            ),
          ),
          const SizedBox(height: 2),
          Text.rich(
            TextSpan(
              style: const TextStyle(fontFamily: 'monospace', fontSize: 12),
              children: spans,
            ),
            maxLines: 3,
            overflow: TextOverflow.ellipsis,
          ),
        ],
      ),
    );
  }
}
