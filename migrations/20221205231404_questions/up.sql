begin;

create sequence pin.question_priority_seq;

create table pin.question_kind
(
    kind text primary key
);

insert into pin.question_kind (kind)
values ('multi');

create table pin.questions
(
    id       bigint primary key   default pin.id_gen(),
    kind     text        not null references pin.question_kind (kind),
    prompt   text        not null,
    used     timestamptz,
    priority bigint      not null default nextval('pin.question_priority_seq'),
    deleted  boolean     not null default false,
    created  timestamptz not null default now(),
    modified timestamptz not null default now()
);
create index idx_questions_used on pin.questions (used);
create index idx_questions_priority on pin.questions (priority);

create table pin.question_multi_options
(
    id          bigint primary key   default pin.id_gen(),
    question_id bigint      not null references pin.questions (id),
    rank        bigint      not null,
    value       text        not null,
    deleted     boolean     not null default false,
    created     timestamptz not null default now(),
    modified    timestamptz not null default now()
);
create index idx_question_multi_options_question on pin.question_multi_options (question_id);
create index idx_question_multi_options_rank on pin.question_multi_options (rank);

with qid as (
    insert into
        pin.questions (kind, prompt)
        values ('multi', '5 poops a day or one poop every 5 days')
    returning id
)
insert into
    pin.question_multi_options (question_id, rank, value)
    values
        ((select id from qid), 0, '5 poops a day'),
        ((select id from qid), 1, 'One poop every 5 days');

commit;