begin;

create unique index idx_pinions_unique_user_question on pin.pinions (user_id, question_id) where deleted is false;

commit;