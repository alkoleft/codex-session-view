import {
  COMPACT_CARD_CONTENT_SPACED_CLASS,
  CardText,
  EventMetaLine,
  EventMetaRow,
  EventSurfaceCard,
  EventTypeLabel,
} from "@/components/session-event-list-common";
import type {
  UserInputOptionEntry,
  UserInputQuestionEntry,
  UserInputRequestEntry,
} from "@/backend";
import type { EventCardTone } from "@/components/session-event-list-tone";
import { cn } from "@/lib/utils";

export function UserInputRequestEventCardView({
  eventLabel,
  metaItems,
  request,
  seqLabel,
  subagentLabel,
  tone,
  timestampLabel,
}: {
  eventLabel: string;
  metaItems: Array<{ label: string; value: string }>;
  request: UserInputRequestEntry;
  seqLabel: string;
  subagentLabel: string | null;
  tone: EventCardTone;
  timestampLabel: string;
}) {
  return (
    <EventSurfaceCard
      contentClassName={COMPACT_CARD_CONTENT_SPACED_CLASS}
      subagentLabel={subagentLabel}
    >
      <div className="flex flex-col gap-1">
        <EventMetaLine
          seqLabel={seqLabel}
          subagentLabel={subagentLabel}
          timestampLabel={timestampLabel}
        />
        <EventTypeLabel label={eventLabel} tone={tone} />
      </div>
      {metaItems.length > 0 ? <EventMetaRow items={metaItems} /> : null}
      <UserInputRequestBlock request={request} />
    </EventSurfaceCard>
  );
}

function UserInputRequestBlock({ request }: { request: UserInputRequestEntry }) {
  const extraAnswers = request.extra_answers.filter(
    (answer) => answer.id.trim() && userInputAnswerValues(answer.answers).length > 0,
  );

  return (
    <div className="flex flex-col gap-3">
      {request.questions.length > 0 ? (
        <div className="flex flex-col gap-2">
          <div className="text-[11px] font-semibold uppercase tracking-[0.08em] text-muted-foreground">
            questions
          </div>
          <div className="flex flex-col gap-3">
            {request.questions.map((question, index) => (
              <UserInputQuestionBlock
                key={question.id ?? `${question.header ?? "question"}-${index}`}
                question={question}
              />
            ))}
          </div>
        </div>
      ) : null}
      {extraAnswers.length > 0 ? (
        <div className="flex flex-col gap-2">
          <div className="text-[11px] font-semibold uppercase tracking-[0.08em] text-muted-foreground">
            answers
          </div>
          <div className="grid gap-2">
            {extraAnswers.map((answer) => (
              <div
                className="grid gap-1 rounded-xl border border-border/60 bg-muted/10 px-3 py-2 sm:grid-cols-[minmax(0,180px)_minmax(0,1fr)] sm:items-start sm:gap-3"
                key={`${answer.id}-${answer.answers.join("|")}`}
              >
                <code className="ui-selectable whitespace-pre-wrap break-words text-[11px] text-muted-foreground">
                  {answer.id}
                </code>
                <UserInputAnswerChips answers={answer.answers} selected={false} />
              </div>
            ))}
          </div>
        </div>
      ) : null}
    </div>
  );
}

function UserInputQuestionBlock({ question }: { question: UserInputQuestionEntry }) {
  const unmatchedAnswers = userInputAnswerValues(question.answers).filter(
    (answer) =>
      !question.options.some((option) => option.label.trim() === answer.trim()),
  );

  return (
    <div className="flex flex-col gap-3 rounded-2xl border border-border/60 bg-muted/10 p-3">
      <div className="flex flex-wrap items-start gap-3">
        <div className="min-w-0 flex-1">
          {question.question?.trim() ? <CardText text={question.question.trim()} tone="default" /> : null}
        </div>
        <div className="ml-auto flex flex-wrap items-center justify-end gap-1.5">
          {question.header?.trim() ? (
            <span className="inline-flex items-center rounded-full border border-border/60 bg-background/60 px-2 py-0.5 text-[10px] uppercase tracking-[0.08em] text-muted-foreground">
              {question.header.trim()}
            </span>
          ) : null}
          {question.id?.trim() ? (
            <span className="inline-flex items-center rounded-full border border-border/60 bg-background/60 px-2 py-0.5 text-[10px] text-muted-foreground">
              <code className="ui-selectable">{question.id.trim()}</code>
            </span>
          ) : null}
        </div>
      </div>
      {question.options.length > 0 ? (
        <div className="flex flex-col gap-2">
          {question.options.map((option, index) => {
            const isSelected = question.answers.some(
              (answer) => answer.trim() === option.label.trim(),
            );
            return (
              <UserInputOptionBlock
                isSelected={isSelected}
                key={`${option.label}-${index}`}
                option={option}
              />
            );
          })}
        </div>
      ) : null}
      {unmatchedAnswers.length > 0 ? (
        <div className="flex flex-col gap-1">
          <div className="text-[11px] font-semibold uppercase tracking-[0.08em] text-muted-foreground">
            answers
          </div>
          <UserInputAnswerChips answers={unmatchedAnswers} selected />
        </div>
      ) : null}
    </div>
  );
}

function UserInputOptionBlock({
  option,
  isSelected,
}: {
  option: UserInputOptionEntry;
  isSelected: boolean;
}) {
  return (
    <div
      className={cn(
        "flex flex-col gap-2 rounded-xl border px-3 py-2",
        isSelected
          ? "border-emerald-500/40 bg-emerald-500/10"
          : "border-border/60 bg-background/40",
      )}
    >
      <div className="flex flex-wrap items-start justify-between gap-2">
        <div className="ui-selectable whitespace-pre-wrap break-words text-sm font-medium text-foreground">
          {option.label}
        </div>
        {isSelected ? (
          <span className="inline-flex items-center rounded-full border border-emerald-500/35 bg-emerald-500/12 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-[0.08em] text-emerald-700 dark:text-emerald-300">
            selected
          </span>
        ) : null}
      </div>
      {option.description?.trim() ? <CardText text={option.description.trim()} tone="muted" /> : null}
    </div>
  );
}

function UserInputAnswerChips({
  answers,
  selected,
}: {
  answers: string[];
  selected: boolean;
}) {
  const values = userInputAnswerValues(answers);
  if (values.length === 0) {
    return null;
  }

  return (
    <div className="flex flex-wrap gap-1.5">
      {values.map((answer, index) => (
        <span
          className={cn(
            "ui-selectable inline-flex items-center rounded-full border px-2 py-0.5 text-xs",
            selected
              ? "border-emerald-500/35 bg-emerald-500/12 text-emerald-700 dark:text-emerald-300"
              : "border-border/60 bg-background/60 text-muted-foreground",
          )}
          key={`${index}-${answer}`}
        >
          {answer}
        </span>
      ))}
    </div>
  );
}

function userInputAnswerValues(answers: string[]) {
  return answers.filter((answer) => answer.trim());
}
