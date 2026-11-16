import useGetMessages from "../../hooks/useGetMessages";
import useSendMessage from "../../hooks/useSendMessage";
import MessagesSkeleton from "../skeletons/MessagesSkeleton/MessagesSkeleton";
import Message from "./Message";
import useServerStore from "../../zustand/useServerStore";
import { useCallback, useLayoutEffect, useMemo, useRef } from "react";
import { usePopupContext } from "../popups/PopupContext";

const Messages = () => {
  const { loading, hasMore, loadOlder } = useGetMessages();
  const { retryMessage } = useSendMessage();
  const messages = useServerStore((state) => state.messages);

  // const usersMap = useMemo(
  //   () =>
  //     users.reduce((acc, user) => {
  //       acc[user.user_id] = user;
  //       return acc;
  //     }, {}),
  //   [users],
  // );

  // const messagesWithUsers = useMemo(() => {
  //   return messages.map((message) => {
  //     const formatted_date = formatTime(message.timestamp);

  //     const { display_username, profile_picture } = usersMap[message.user_id];
  //     return {
  //       ...message,
  //       display_username,
  //       profile_picture,
  //       formatted_date,
  //     };
  //   });
  // }, [usersMap, messages]);

  // dont render messages if loading, cause it would be the previous channel's messages
  // maybe add message cacheing

  // TODO: put functions outside with the mapper
  // TODO: fix the handle submit functions by just adding the event parameter with it
  const { setUserMenu, setModalOverlayVisible } = usePopupContext();

  const handleClick = useCallback(
    (event, user_id, display_username, s3_icon_key) => {
      event.stopPropagation();
      setModalOverlayVisible(true);
      setUserMenu({
        visible: true,
        user: {
          user_id,
          display_username,
          s3_icon_key,
        },
      });
    },
    [setModalOverlayVisible, setUserMenu],
  );

  const mapMessages = useMemo(
    () =>
      messages.map((message) => {
        return (
          <Message
            message={message}
            key={message.nonce ?? message.message_id}
            handleClick={(event) =>
              handleClick(
                event,
                message.user_id,
                message.display_username,
                message.s3_icon_key,
              )
            }
            onRetry={() => retryMessage(message)}
          />
        );
      }),
    [messages, handleClick, retryMessage],
  );

  const scrollRef = useRef(null);
  // distance from the bottom saved before prepending an older page
  const anchorRef = useRef(null);
  const firstKeyRef = useRef(null);

  const handleScroll = useCallback(async () => {
    const el = scrollRef.current;
    if (loading || !hasMore || anchorRef.current !== null || el.scrollTop > 200)
      return;
    anchorRef.current = el.scrollHeight - el.scrollTop;
    if ((await loadOlder()) === 0) anchorRef.current = null;
  }, [loading, hasMore, loadOlder]);

  // after a prepend keep the view where it was, otherwise stick to the bottom
  useLayoutEffect(() => {
    const el = scrollRef.current;
    if (loading) anchorRef.current = null; // channel switched mid-load
    const firstKey = messages[0]?.nonce ?? messages[0]?.message_id;
    if (anchorRef.current === null) {
      el.scrollTop = el.scrollHeight;
    } else if (firstKey !== firstKeyRef.current) {
      el.scrollTop = el.scrollHeight - anchorRef.current;
      anchorRef.current = null;
    }
    firstKeyRef.current = firstKey;
    // no scrollbar means no scroll event, so keep loading until it overflows
    if (el.scrollHeight <= el.clientHeight) handleScroll();
  }, [messages, loading, handleScroll]);

  return (
    <div
      ref={scrollRef}
      onScroll={handleScroll}
      className="scrollbar-messages flex flex-1 flex-col overflow-x-auto overflow-y-scroll rounded-md bg-base-100 px-2"
    >
      <div className="flex-1"></div>
      {loading ? <MessagesSkeleton /> : mapMessages}
      <div className="min-h-4 w-[1px]"></div>
    </div>
  );
};

export default Messages;
