begin;

create index idx_questions_created on pin.questions (created);

commit;
