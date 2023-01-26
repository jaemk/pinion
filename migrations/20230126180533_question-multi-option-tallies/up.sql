begin;

create table pin.question_multi_option_tallies
(
    id              bigint primary key   default pin.id_gen(),
    question_id     bigint      not null references pin.questions (id),
    multi_selection bigint      not null references pin.question_multi_options (id),
    count           bigint      not null default 0,
    deleted         boolean     not null default false,
    created         timestamptz not null default now(),
    modified        timestamptz not null default now()
);
create index idx_question_multi_option_tallies_question on pin.question_multi_option_tallies (question_id);
create index idx_question_multi_option_tallies_multi_selection on pin.question_multi_option_tallies (multi_selection);
create unique index idx_question_multi_option_tallies_question_selection on pin.question_multi_option_tallies (question_id, multi_selection) where deleted is false;

commit;